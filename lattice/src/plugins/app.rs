use std::rc::Rc;

use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Ctx, Exception, Function, JsLifetime, Value};

// The `sol:app` module: the running application's own surface. exit() ends
// the current app instance, background() leaves it to the OS without ending
// it. What either means is the host's policy (see ExitPolicy in lib.rs): a
// client with an app running returns to the player for both; at the player
// root and in the standalone runtime, exit quits (on Android by finishing
// the activity) and background hands the client to the OS (moveTaskToBack
// on Android, a minimized window on desktop). The quit hook is not run
// here: core's exit() wrapper dispatches onQuit and calls the verb once the
// handlers settled, so this stays the bare native end. setCadenceHold() and
// resetCadenceHold() are the app's layer of the cadence-hold policy
// (cadence.rs). Not the player-only store surface - that is `sol:apps`.

// The app control installed as context userdata: the engine-agnostic
// closures, so this module never references runner policy directly.
#[derive(Clone, JsLifetime)]
pub struct AppControl(#[qjs(skip_trace)] Rc<AppControlInner>);

pub struct AppControlInner {
  pub exit: Box<dyn Fn()>,
  pub background: Box<dyn Fn()>,
  /// The app's cadence-hold choice; None hands it back to the default.
  pub set_cadence_hold: Box<dyn Fn(Option<alloy::CadenceHold>)>,
  pub reset_cadence_hold: Box<dyn Fn()>,
}

impl AppControl {
  pub fn new(inner: AppControlInner) -> Self {
    Self(Rc::new(inner))
  }
}

// Installs the app control as userdata. Wired from lib.rs on every engine
// build (all builds, unlike the go-only sol:apps control).
pub fn install(ctx: &Ctx<'_>, control: AppControl) {
  ctx.store_userdata(control).expect("store app control");
}

fn exit_impl(ctx: Ctx<'_>) -> flux::rquickjs::Result<()> {
  let Some(control) = ctx.userdata::<AppControl>().map(|c| c.clone()) else {
    return Err(Exception::throw_message(&ctx, "sol:app is not available in this build"));
  };
  (control.0.exit)();
  Ok(())
}

fn background_impl(ctx: Ctx<'_>) -> flux::rquickjs::Result<()> {
  let Some(control) = ctx.userdata::<AppControl>().map(|c| c.clone()) else {
    return Err(Exception::throw_message(&ctx, "sol:app is not available in this build"));
  };
  (control.0.background)();
  Ok(())
}

// setCadenceHold(policy): "off", "auto", a whole number of refreshes (at
// least 1), or "default" (the choice back to the runtime). The applied state
// comes back on the sticky "cadenceHold" event.
fn set_cadence_hold_impl<'js>(ctx: Ctx<'js>, policy: Value<'js>) -> flux::rquickjs::Result<()> {
  let Some(control) = ctx.userdata::<AppControl>().map(|c| c.clone()) else {
    return Err(Exception::throw_message(&ctx, "sol:app is not available in this build"));
  };
  let expected = |got: String| {
    Exception::throw_message(
      &ctx,
      &format!("setCadenceHold: expected \"off\", \"auto\", \"default\" or a whole number of refreshes, got {got}"),
    )
  };
  let hold = if let Some(name) = policy.as_string() {
    match name.to_string()?.as_str() {
      "off" => Some(alloy::CadenceHold::Off),
      "auto" => Some(crate::cadence::auto()),
      "default" => None,
      other => return Err(expected(format!("\"{other}\""))),
    }
  } else if let Some(n) = policy.as_number() {
    if n.fract() != 0.0 || n < 1.0 || n > u32::MAX as f64 {
      return Err(expected(n.to_string()));
    }
    Some(alloy::CadenceHold::Fixed(n as u32))
  } else {
    return Err(expected(policy.type_name().to_string()));
  };
  (control.0.set_cadence_hold)(hold);
  Ok(())
}

// resetCadenceHold(): end the hold the runtime learned, at once, keeping the
// policy.
fn reset_cadence_hold_impl(ctx: Ctx<'_>) -> flux::rquickjs::Result<()> {
  let Some(control) = ctx.userdata::<AppControl>().map(|c| c.clone()) else {
    return Err(Exception::throw_message(&ctx, "sol:app is not available in this build"));
  };
  (control.0.reset_cadence_hold)();
  Ok(())
}

pub struct SolAppModule;

impl ModuleDef for SolAppModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("exit")?;
    decl.declare("background")?;
    decl.declare("setCadenceHold")?;
    decl.declare("resetCadenceHold")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    exports.export("exit", Function::new(ctx.clone(), exit_impl)?)?;
    exports.export("background", Function::new(ctx.clone(), background_impl)?)?;
    exports.export("setCadenceHold", Function::new(ctx.clone(), set_cadence_hold_impl)?)?;
    exports.export("resetCadenceHold", Function::new(ctx.clone(), reset_cadence_hold_impl)?)?;
    Ok(())
  }
}
