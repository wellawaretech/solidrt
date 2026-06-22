use std::rc::Rc;

use flux::rquickjs::function::MutFn;
use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Array, Ctx, Function, JsLifetime, Null};

// The `srt:dev` module: the dev-server control surface (connect / discover /
// stop) used by the default app's connection UI. The actual command plumbing
// lives in the go layer (connection.rs); here we only marshal it to JS.
//
// The module is registered unconditionally so the default app's static import
// resolves in every build, but the underlying control is installed only in go
// builds via `install`. When absent (record / non-go runtimes), `available` is
// false and the functions are no-ops, which is what the default app guards on.

// The dev control installed as context userdata. Holds engine-agnostic closures
// so this module never references the go-only command channel directly.
#[derive(Clone, JsLifetime)]
pub struct DevControl(#[qjs(skip_trace)] Rc<DevControlInner>);

pub struct DevControlInner {
  pub connect: Box<dyn Fn(String)>,
  pub discover: Box<dyn Fn()>,
  pub stop: Box<dyn Fn()>,
  pub can_discover: bool,
  pub recents: Vec<String>,
  // Dev-server address delivered at launch (srt client --android); the default
  // app auto-connects to it. None when launched without one.
  pub launch_address: Option<String>,
}

impl DevControl {
  pub fn new(inner: DevControlInner) -> Self {
    Self(Rc::new(inner))
  }
}

// Installs the dev control as userdata. Call from a go engine plugin before the
// default app imports `srt:dev`.
pub fn install(ctx: &Ctx<'_>, control: DevControl) {
  ctx.store_userdata(control).expect("store dev control");
}

pub struct SrtDevModule;

impl ModuleDef for SrtDevModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("available")?;
    decl.declare("connect")?;
    decl.declare("discover")?;
    decl.declare("stop")?;
    decl.declare("canDiscover")?;
    decl.declare("recents")?;
    decl.declare("launchAddress")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    match ctx.userdata::<DevControl>() {
      Some(control) => {
        let connect = control.clone();
        let discover = control.clone();
        let stop = control.clone();

        exports.export("available", true)?;
        exports.export(
          "connect",
          Function::new(ctx.clone(), MutFn::from(move |addr: String| (connect.0.connect)(addr)))?,
        )?;
        exports.export("discover", Function::new(ctx.clone(), MutFn::from(move || (discover.0.discover)()))?)?;
        exports.export("stop", Function::new(ctx.clone(), MutFn::from(move || (stop.0.stop)()))?)?;
        exports.export("canDiscover", control.0.can_discover)?;

        let recents = Array::new(ctx.clone())?;
        for (i, addr) in control.0.recents.iter().enumerate() {
          recents.set(i, addr.clone())?;
        }
        exports.export("recents", recents)?;
        match &control.0.launch_address {
          Some(addr) => exports.export("launchAddress", addr.clone())?,
          None => exports.export("launchAddress", Null)?,
        };
      }
      None => {
        exports.export("available", false)?;
        exports.export("connect", Function::new(ctx.clone(), MutFn::from(|_: String| {}))?)?;
        exports.export("discover", Function::new(ctx.clone(), MutFn::from(|| {}))?)?;
        exports.export("stop", Function::new(ctx.clone(), MutFn::from(|| {}))?)?;
        exports.export("canDiscover", false)?;
        exports.export("recents", Array::new(ctx.clone())?)?;
        exports.export("launchAddress", Null)?;
      }
    }
    Ok(())
  }
}
