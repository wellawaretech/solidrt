use std::rc::Rc;

use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Array, Ctx, Exception, Function, JsLifetime, Object};

// The `srt:apps` module: the launcher's surface over the client's version
// store (list / launch / remove installed apps). The store logic lives in the
// go layer (go/store.rs); here we only marshal it to JS.
//
// The module is registered unconditionally so the launcher's static import
// resolves in every build, but the underlying control is installed only in go
// builds. When absent, `available` is false, `list` returns [], and
// launch/remove are no-ops, matching `srt:dev`.

/// One installed app as `list()` returns it.
pub struct AppEntry {
  pub id: String,
  pub name: String,
  pub version: String,
}

/// A stored version as `info()` lists it.
pub struct AppVersion {
  pub id: String,
  pub size: u64,
  pub current: bool,
}

/// One file in a listing: a relative path and its size in bytes.
pub struct AppFile {
  pub path: String,
  pub size: u64,
}

/// Usage details as `info()` returns them.
pub struct AppInfo {
  pub id: String,
  pub name: String,
  pub version: String,
  pub install_size: u64,
  pub data_size: u64,
  pub versions: Vec<AppVersion>,
  /// Manifest-declared assets of the current version.
  pub assets: Vec<AppFile>,
  /// The current version dir's actual files on disk.
  pub files: Vec<AppFile>,
  /// The data sandbox's actual files on disk.
  pub data: Vec<AppFile>,
}

// The apps control installed as context userdata. Holds engine-agnostic
// closures so this module never references the go-only store directly.
#[derive(Clone, JsLifetime)]
pub struct AppsControl(#[qjs(skip_trace)] Rc<AppsControlInner>);

pub struct AppsControlInner {
  pub list: Box<dyn Fn() -> Vec<AppEntry>>,
  pub info: Box<dyn Fn(String) -> Result<AppInfo, String>>,
  pub launch: Box<dyn Fn(String) -> Result<(), String>>,
  pub remove: Box<dyn Fn(String) -> Result<(), String>>,
}

impl AppsControl {
  pub fn new(inner: AppsControlInner) -> Self {
    Self(Rc::new(inner))
  }
}

// Installs the apps control as userdata. Call from a go engine plugin before
// the launcher imports `srt:apps`.
pub fn install(ctx: &Ctx<'_>, control: AppsControl) {
  ctx.store_userdata(control).expect("store apps control");
}

fn list_impl<'js>(ctx: Ctx<'js>) -> flux::rquickjs::Result<Array<'js>> {
  let apps = Array::new(ctx.clone())?;
  let Some(control) = ctx.userdata::<AppsControl>().map(|c| c.clone()) else { return Ok(apps) };
  for (i, app) in (control.0.list)().into_iter().enumerate() {
    let entry = Object::new(ctx.clone())?;
    entry.set("id", app.id)?;
    entry.set("name", app.name)?;
    entry.set("version", app.version)?;
    apps.set(i, entry)?;
  }
  Ok(apps)
}

// Sizes cross into JS as f64: exact up to 2^53 bytes, far beyond any app.
fn info_impl<'js>(ctx: Ctx<'js>, id: String) -> flux::rquickjs::Result<Object<'js>> {
  let Some(control) = ctx.userdata::<AppsControl>().map(|c| c.clone()) else {
    return Err(Exception::throw_message(&ctx, "srt:apps is not available in this build"));
  };
  let info = (control.0.info)(id).map_err(|m| Exception::throw_message(&ctx, &m))?;
  let obj = Object::new(ctx.clone())?;
  obj.set("id", info.id)?;
  obj.set("name", info.name)?;
  obj.set("version", info.version)?;
  obj.set("installSize", info.install_size as f64)?;
  obj.set("dataSize", info.data_size as f64)?;
  let versions = Array::new(ctx.clone())?;
  for (i, v) in info.versions.into_iter().enumerate() {
    let entry = Object::new(ctx.clone())?;
    entry.set("id", v.id)?;
    entry.set("size", v.size as f64)?;
    entry.set("current", v.current)?;
    versions.set(i, entry)?;
  }
  obj.set("versions", versions)?;
  let file_list = |files: Vec<AppFile>| -> flux::rquickjs::Result<Array<'js>> {
    let arr = Array::new(ctx.clone())?;
    for (i, f) in files.into_iter().enumerate() {
      let entry = Object::new(ctx.clone())?;
      entry.set("path", f.path)?;
      entry.set("size", f.size as f64)?;
      arr.set(i, entry)?;
    }
    Ok(arr)
  };
  obj.set("assets", file_list(info.assets)?)?;
  obj.set("files", file_list(info.files)?)?;
  obj.set("data", file_list(info.data)?)?;
  Ok(obj)
}

fn launch_impl(ctx: Ctx<'_>, id: String) -> flux::rquickjs::Result<()> {
  let Some(control) = ctx.userdata::<AppsControl>().map(|c| c.clone()) else { return Ok(()) };
  (control.0.launch)(id).map_err(|m| Exception::throw_message(&ctx, &m))
}

fn remove_impl(ctx: Ctx<'_>, id: String) -> flux::rquickjs::Result<()> {
  let Some(control) = ctx.userdata::<AppsControl>().map(|c| c.clone()) else { return Ok(()) };
  (control.0.remove)(id).map_err(|m| Exception::throw_message(&ctx, &m))
}

pub struct SrtAppsModule;

impl ModuleDef for SrtAppsModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("available")?;
    decl.declare("list")?;
    decl.declare("info")?;
    decl.declare("launch")?;
    decl.declare("remove")?;
    decl.declare("version")?;
    decl.declare("profile")?;
    decl.declare("platform")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    exports.export("available", ctx.userdata::<AppsControl>().is_some())?;
    exports.export("list", Function::new(ctx.clone(), list_impl)?)?;
    exports.export("info", Function::new(ctx.clone(), info_impl)?)?;
    exports.export("launch", Function::new(ctx.clone(), launch_impl)?)?;
    exports.export("remove", Function::new(ctx.clone(), remove_impl)?)?;
    // Build identity of this runtime, for the launcher's settings screen. Not
    // app-specific, but the launcher already imports this module.
    exports.export("version", crate::VERSION)?;
    exports.export("profile", crate::PROFILE)?;
    exports.export("platform", std::env::consts::OS)?;
    Ok(())
  }
}
