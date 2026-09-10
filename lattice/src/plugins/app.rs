use std::rc::Rc;

use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Ctx, Exception, Function, JsLifetime};

// The `srt:app` module: the running application's own surface. Two verbs:
// exit() ends the current app instance, background() leaves it to the OS
// without ending it. What either means is the host's policy (see
// ExitPolicy in lib.rs): a client with an app running returns to the player
// for both; at the player root and in the standalone runtime, exit quits
// (on Android by finishing the activity) and background hands the client to
// the OS (moveTaskToBack on Android, a minimized window on desktop). The
// quit hook is not run here: core's exit() wrapper dispatches onQuit and
// calls the verb once the handlers settled, so this stays the bare native
// end. Not the player-only store surface - that is `srt:apps`.

// The app control installed as context userdata: the engine-agnostic exit
// closure, so this module never references runner policy directly.
#[derive(Clone, JsLifetime)]
pub struct AppControl(#[qjs(skip_trace)] Rc<AppControlInner>);

pub struct AppControlInner {
  pub exit: Box<dyn Fn()>,
  pub background: Box<dyn Fn()>,
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

fn background_impl(ctx: Ctx<'_>) -> flux::rquickjs::Result<()> {
  let Some(control) = ctx.userdata::<AppControl>().map(|c| c.clone()) else {
    return Err(Exception::throw_message(&ctx, "srt:app is not available in this build"));
  };
  (control.0.background)();
  Ok(())
}

pub struct SrtAppModule;

impl ModuleDef for SrtAppModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("exit")?;
    decl.declare("background")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    exports.export("exit", Function::new(ctx.clone(), exit_impl)?)?;
    exports.export("background", Function::new(ctx.clone(), background_impl)?)?;
    Ok(())
  }
}
