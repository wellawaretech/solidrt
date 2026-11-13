//! `flux:haptics`: `feedback`, `stopFeedback` and `feedbackSupport`, a thin marshalling
//! layer over `ore::haptics`'s `Session`, the session this engine keeps.
//! The app names what happened (an intent), and the platform plays its
//! own effect for it; every call is accepted everywhere and is a no-op
//! where nothing plays. The session logs every call, which the embedder
//! reads back for a test or an agent, and reaches no platform when set to
//! record only.

use std::sync::Arc;

use ore::haptics::{Call, Impact, Intent, Session, Weight};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Exception, Function, JsLifetime, Object, Value};

use crate::plugins::marshal::OptArg;

/// The capability haptics is listed under, only where the device has an
/// actuator (`ore::haptics::available`), or where the session records
/// only.
pub(crate) const CAPABILITY: &str = "haptics";

/// The lowest and highest `intensity` an impact takes.
const MIN_INTENSITY: f64 = 0.0;
const MAX_INTENSITY: f64 = 1.0;

#[derive(Clone, JsLifetime)]
struct HapticsState(#[qjs(skip_trace)] Arc<Session>);

/// Store the haptics session in userdata. Runs at engine init, before any
/// module import.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(HapticsState(Session::new())).expect("store haptics state");
}

/// This engine's haptics session, for the embedder's test verb and
/// control surface (the feedback calls the app made).
pub fn session(ctx: &Ctx<'_>) -> Option<Arc<Session>> {
  ctx.userdata::<HapticsState>().map(|s| s.0.clone())
}

/// Never reach the platform in this engine: every call is logged and
/// plays nothing, and support reports a device that plays every intent (a
/// test host's engine, a render). The engine then lists the "haptics"
/// capability whether or not the device has an actuator.
pub fn set_record_only(ctx: &Ctx<'_>) {
  let Some(session) = session(ctx) else {
    return;
  };
  if session.is_record_only() {
    return;
  }
  session.set_record_only();
  if !ore::haptics::available() {
    crate::add_capability(ctx, CAPABILITY);
  }
}

/// The `flux:haptics` module.
pub struct HapticsModule;

impl ModuleDef for HapticsModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("feedback")?;
    decl.declare("stopFeedback")?;
    decl.declare("feedbackSupport")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("feedback", Function::new(ctx.clone(), feedback)?)?;
    exports.export("stopFeedback", Function::new(ctx.clone(), stop_feedback)?)?;
    exports.export("feedbackSupport", Function::new(ctx.clone(), feedback_support)?)?;
    Ok(())
  }
}

/// The intent names, for an error that lists the valid ones.
fn intent_names() -> String {
  Intent::ALL.iter().map(|intent| intent.name()).collect::<Vec<_>>().join(", ")
}

/// `feedback(intent, options?)`: the intent by name, and for `impact` the
/// options' `weight` and `intensity`, handed to the session.
fn feedback<'js>(ctx: Ctx<'js>, intent: Value<'js>, options: OptArg<Value<'js>>) -> rquickjs::Result<()> {
  let Some(name) = intent.as_string().map(|s| s.to_string()).transpose()? else {
    return Err(Exception::throw_type(
      &ctx,
      &format!("feedback: pass the intent as a string, one of {}", intent_names()),
    ));
  };
  let Some(intent) = Intent::from_name(&name) else {
    return Err(Exception::throw_type(&ctx, &format!("feedback: unknown intent {name:?}; one of {}", intent_names())));
  };
  let impact = match options.0 {
    Some(options) => impact_of(&ctx, options)?,
    None => Impact::default(),
  };
  let Some(session) = session(&ctx) else {
    return Err(Exception::throw_message(&ctx, "feedback: no haptics session in this engine"));
  };
  session.feedback(Call { intent, impact: Some(impact) });
  Ok(())
}

/// The impact `options` names, the defaults for what it leaves out.
fn impact_of<'js>(ctx: &Ctx<'js>, options: Value<'js>) -> rquickjs::Result<Impact> {
  let Some(options) = options.as_object() else {
    return Err(Exception::throw_type(ctx, "feedback: options must be an object"));
  };
  let mut impact = Impact::default();
  let weight: Value = options.get("weight")?;
  if !weight.is_undefined() && !weight.is_null() {
    let Some(name) = weight.as_string().map(|s| s.to_string()).transpose()? else {
      return Err(Exception::throw_type(ctx, "feedback: weight must be a string, one of light, medium, heavy"));
    };
    let Some(weight) = Weight::from_name(&name) else {
      return Err(Exception::throw_type(
        ctx,
        &format!("feedback: unknown weight {name:?}; one of light, medium, heavy"),
      ));
    };
    impact.weight = weight;
  }
  let intensity: Value = options.get("intensity")?;
  if !intensity.is_undefined() && !intensity.is_null() {
    match intensity.as_number() {
      Some(value) if value.is_finite() && (MIN_INTENSITY..=MAX_INTENSITY).contains(&value) => {
        impact.intensity = value as f32;
      }
      _ => {
        return Err(Exception::throw_range(ctx, "feedback: intensity must be a finite number in 0..1"));
      }
    }
  }
  Ok(impact)
}

/// `stopFeedback()`: cancel what plays now.
fn stop_feedback(ctx: Ctx<'_>) {
  if let Some(session) = session(&ctx) {
    session.stop();
  }
}

/// `feedbackSupport()`: `{ actuator, intents: { <intent>: <level> }, impactIntensity }`
/// from the session's facts, frozen (the intents object too).
fn feedback_support<'js>(ctx: Ctx<'js>) -> rquickjs::Result<Object<'js>> {
  let facts = match session(&ctx) {
    Some(session) => session.support(),
    None => ore::haptics::Support::none(),
  };
  let intents = Object::new(ctx.clone())?;
  for intent in Intent::ALL {
    intents.set(intent.name(), facts.level(intent).name())?;
  }
  freeze(&ctx, &intents)?;
  let support = Object::new(ctx.clone())?;
  support.set("actuator", facts.actuator)?;
  support.set("intents", intents)?;
  support.set("impactIntensity", facts.impact_intensity)?;
  freeze(&ctx, &support)?;
  Ok(support)
}

/// `Object.freeze(object)`.
fn freeze<'js>(ctx: &Ctx<'js>, object: &Object<'js>) -> rquickjs::Result<()> {
  let global: Object = ctx.globals().get("Object")?;
  let freeze: Function = global.get("freeze")?;
  freeze.call::<_, ()>((object.clone(),))
}
