//! `Flux.availability(name)` and the sticky `availability` event: whether
//! this device, platform and session provide a platform facility now, a
//! thin marshalling layer over `ore::availability`. ore owns the facts;
//! this engine keeps only its overrides (a test host's session that never
//! reaches the platform reports the facility available in that engine),
//! applied over ore's states in the snapshot. The event carries the whole
//! snapshot, every facility present, and is emitted once after the plugin
//! chain and again whenever a state changes.

use std::cell::RefCell;
use std::collections::HashMap;

use ore::availability::{Availability, Facility};
use rquickjs::{Ctx, Exception, Function, JsLifetime, Object, Value};

use crate::plugins::events::emit_sticky;

/// The sticky event the snapshot travels on.
const EVENT: &str = "availability";

const API: &str = "Flux.availability";

/// This engine's overrides: a facility's state as this session makes it,
/// whatever ore says.
#[derive(Default, JsLifetime)]
struct Overrides(#[qjs(skip_trace)] RefCell<HashMap<Facility, Availability>>);

/// Store the engine's (empty) overrides in userdata. Runs at engine init,
/// before any plugin.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(Overrides::default()).expect("store availability overrides");
}

/// Report `facility` as `state` in this engine, whatever the platform says
/// (a test host's session that never reaches the platform). Call `emit`
/// afterwards for a reader of the event to see it.
pub fn set_override(ctx: &Ctx<'_>, facility: Facility, state: Availability) {
  if let Some(overrides) = ctx.userdata::<Overrides>() {
    overrides.0.borrow_mut().insert(facility, state);
  }
}

/// The availability of `facility` in this engine: Unavailable when the
/// app's project declared it does not use it (`crate::capability_excluded`;
/// the app is to hide the feature, whatever the device has), else its
/// override, else ore's.
pub fn of(ctx: &Ctx<'_>, facility: Facility) -> Availability {
  if crate::capability_excluded(ctx, facility.name()) {
    return Availability::Unavailable;
  }
  let overridden = ctx.userdata::<Overrides>().and_then(|overrides| overrides.0.borrow().get(&facility).copied());
  overridden.unwrap_or_else(|| ore::availability::of(facility))
}

/// Every facility's name and state name in this engine, in
/// `Facility::ALL` order (for an embedder's control surface).
pub fn snapshot(ctx: &Ctx<'_>) -> Vec<(&'static str, &'static str)> {
  Facility::ALL.iter().map(|facility| (facility.name(), of(ctx, *facility).name())).collect()
}

/// The facility named `name` ("link", "clipboard-image", ...), or None.
pub fn facility_named(name: &str) -> Option<Facility> {
  Facility::ALL.iter().copied().find(|facility| facility.name() == name)
}

/// The state named `name` ("available", "unavailable", "unknown"), or None.
pub fn state_named(name: &str) -> Option<Availability> {
  [Availability::Available, Availability::Unavailable, Availability::Unknown]
    .into_iter()
    .find(|state| state.name() == name)
}

/// Emit the sticky `availability` event with this engine's snapshot, an
/// object of facility name to state name.
pub fn emit(ctx: &Ctx<'_>) {
  let payload = Object::new(ctx.clone()).expect("create availability object");
  for (name, state) in snapshot(ctx) {
    payload.set(name, state).expect("set availability state");
  }
  emit_sticky(ctx, EVENT, payload);
}

/// Define `Flux.availability` on `flux`. Runs before `Flux` is frozen.
pub(crate) fn init<'js>(ctx: &Ctx<'js>, flux: &Object<'js>) {
  flux
    .set("availability", Function::new(ctx.clone(), availability).expect("create Flux.availability"))
    .expect("set Flux.availability");
}

/// `Flux.availability(name)`: the named facility's state name.
fn availability<'js>(ctx: Ctx<'js>, name: Value<'js>) -> rquickjs::Result<&'static str> {
  let Some(name) = name.as_string().map(|s| s.to_string()).transpose()? else {
    return Err(Exception::throw_type(&ctx, &format!("{API}: pass the facility name as a string")));
  };
  let Some(facility) = facility_named(&name) else {
    return Err(Exception::throw_type(&ctx, &format!("{API}: unknown facility {name}")));
  };
  Ok(of(&ctx, facility).name())
}
