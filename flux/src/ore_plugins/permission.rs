//! `Flux.permission(name)` and the sticky `permission` event: the user's
//! standing answer on a device permission, a thin marshalling layer over
//! `ore::permission`. ore owns the states; this engine keeps only its
//! overrides (a test sets one), applied over ore's in every read. The same
//! shape as `availability`: a synchronous read of one name, and the whole
//! map on one sticky event, emitted once after the plugin chain, on every
//! ore change, and by a test's override.
//!
//! The contract (packages/flux-types/index.d.ts): the names are "camera"
//! and "microphone", the state is "granted", "denied" or "prompt". Any
//! other name, and a name the app's project declared it does not use
//! (`crate::capability_excluded`), throws a `TypeError`. There is no
//! request: an open is the request.

use std::cell::RefCell;
use std::collections::HashMap;

use ore::permission::{Permission, PermissionState};
use rquickjs::{Ctx, Exception, Function, JsLifetime, Object, Value};

use crate::plugins::events::emit_sticky;

/// The sticky event the map travels on.
const EVENT: &str = "permission";

const API: &str = "Flux.permission";

/// This engine's overrides: a permission's state as this session makes it,
/// whatever ore says.
#[derive(Default, JsLifetime)]
struct Overrides(#[qjs(skip_trace)] RefCell<HashMap<Permission, PermissionState>>);

/// Store the engine's (empty) overrides in userdata. Runs at engine init,
/// before any plugin.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(Overrides::default()).expect("store permission overrides");
}

/// Report `permission` as `state` in this engine, whatever the platform
/// says (a test's session). Call `emit` afterwards for a reader of the
/// event to see it.
pub fn set_override(ctx: &Ctx<'_>, permission: Permission, state: PermissionState) {
  if let Some(overrides) = ctx.userdata::<Overrides>() {
    overrides.0.borrow_mut().insert(permission, state);
  }
}

/// The state of `permission` in this engine: its override, else ore's.
pub fn of(ctx: &Ctx<'_>, permission: Permission) -> PermissionState {
  let overridden = ctx.userdata::<Overrides>().and_then(|overrides| overrides.0.borrow().get(&permission).copied());
  overridden.unwrap_or_else(|| ore::permission::state(permission))
}

/// Every permission's name and state name in this engine, in
/// `Permission::ALL` order (for an embedder's control surface).
pub fn snapshot(ctx: &Ctx<'_>) -> Vec<(&'static str, &'static str)> {
  Permission::ALL.iter().map(|permission| (permission.name(), of(ctx, *permission).name())).collect()
}

/// The permission named `name` ("camera", "microphone"), or None.
pub fn permission_named(name: &str) -> Option<Permission> {
  Permission::ALL.iter().copied().find(|permission| permission.name() == name)
}

/// The state named `name` ("granted", "denied", "prompt"), or None.
pub fn state_named(name: &str) -> Option<PermissionState> {
  PermissionState::ALL.iter().copied().find(|state| state.name() == name)
}

/// Emit the sticky `permission` event with this engine's map, an object of
/// permission name to state name.
pub fn emit(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
  let payload = Object::new(ctx.clone())?;
  for (name, state) in snapshot(ctx) {
    payload.set(name, state)?;
  }
  emit_sticky(ctx, EVENT, payload);
  Ok(())
}

/// Define `Flux.permission` on `flux`. Runs before `Flux` is frozen.
pub(crate) fn init<'js>(ctx: &Ctx<'js>, flux: &Object<'js>) {
  flux
    .set("permission", Function::new(ctx.clone(), permission).expect("create Flux.permission"))
    .expect("set Flux.permission");
}

/// `Flux.permission(name)`: the named permission's state name. A name the
/// project declared it does not use is refused like an unknown one: it is
/// a packaging fact, not a permission state.
fn permission<'js>(ctx: Ctx<'js>, name: Value<'js>) -> rquickjs::Result<&'static str> {
  let Some(name) = name.as_string().map(|s| s.to_string()).transpose()? else {
    return Err(Exception::throw_type(&ctx, &format!("{API}: pass the permission name as a string")));
  };
  let Some(permission) = permission_named(&name) else {
    return Err(Exception::throw_type(&ctx, &format!("{API}: unknown permission {name}")));
  };
  if crate::capability_excluded(&ctx, permission.name()) {
    return Err(Exception::throw_type(&ctx, &format!("{API}: the project does not use {name}")));
  }
  Ok(of(&ctx, permission).name())
}
