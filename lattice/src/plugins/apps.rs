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

// The apps control installed as context userdata. Holds engine-agnostic
// closures so this module never references the go-only store directly.
#[derive(Clone, JsLifetime)]
pub struct AppsControl(#[qjs(skip_trace)] Rc<AppsControlInner>);

pub struct AppsControlInner {
  pub list: Box<dyn Fn() -> Vec<AppEntry>>,
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
    decl.declare("launch")?;
    decl.declare("remove")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    exports.export("available", ctx.userdata::<AppsControl>().is_some())?;
    exports.export("list", Function::new(ctx.clone(), list_impl)?)?;
    exports.export("launch", Function::new(ctx.clone(), launch_impl)?)?;
    exports.export("remove", Function::new(ctx.clone(), remove_impl)?)?;
    Ok(())
  }
}
