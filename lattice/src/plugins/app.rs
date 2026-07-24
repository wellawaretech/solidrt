use std::rc::Rc;

use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Ctx, Exception, Function, JsLifetime};

// The `srt:app` module: the running application's own surface. One verb
// today: exit() - leave the current app, unconditionally. What leaving means
// is the host's policy (see ExitPolicy in lib.rs): a client with an app
// running returns to the launcher; the launcher root and the standalone
// runtime quit (Android backgrounds the activity instead of dying).
//
// Core's default action for an unprevented `back` event calls exit(); apps
// call it directly to leave programmatically, e.g. after intercepting back
// for an unsaved-changes dialog. Not the launcher-only store surface - that
// is `srt:apps`.

// The app control installed as context userdata: the engine-agnostic exit
// closure, so this module never references runner policy directly.
#[derive(Clone, JsLifetime)]
pub struct AppControl(#[qjs(skip_trace)] Rc<AppControlInner>);

pub struct AppControlInner {
  pub exit: Box<dyn Fn()>,
}

impl AppControl {
  pub fn new(inner: AppControlInner) -> Self {
    Self(Rc::new(inner))
  }
}

// Installs the app control as userdata. Wired from lib.rs on every engine
// build (all builds, unlike the go-only srt:apps control).
pub fn install(ctx: &Ctx<'_>, control: AppControl) {
  ctx.store_userdata(control).expect("store app control");
}

fn exit_impl(ctx: Ctx<'_>) -> flux::rquickjs::Result<()> {
  let Some(control) = ctx.userdata::<AppControl>().map(|c| c.clone()) else {
    return Err(Exception::throw_message(&ctx, "srt:app is not available in this build"));
  };
  (control.0.exit)();
  Ok(())
}

pub struct SrtAppModule;

impl ModuleDef for SrtAppModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("exit")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    exports.export("exit", Function::new(ctx.clone(), exit_impl)?)?;
    Ok(())
  }
}
