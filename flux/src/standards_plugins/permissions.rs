//! `navigator.permissions`: the web's Permissions API, simplified, in every
//! engine, a thin marshalling layer over `ore::permission`. ore owns the
//! states; this engine keeps only its overrides (a test sets one), applied
//! over ore's in every read. The surface is plain JS (permissions.js); this
//! module hands it two hooks and installs what it returns.
//!
//! The simplified contract (packages/flux-types/standards/permissions.d.ts):
//! - `query({ name })` resolves to a `PermissionStatus` with `name`, a live
//!   `state` ("granted" | "denied" | "prompt"), `onchange` and
//!   `addEventListener("change")`/`removeEventListener`. The names are
//!   "camera" and "microphone". Any other name, a non-object argument, a
//!   missing name, and a name the app's project declared it does not use
//!   (`crate::capability_excluded`) reject with a `TypeError`, as the web
//!   rejects a name it does not support.
//! - There is no `request()`: an open is the request.
//! - Changes travel as the sticky bus event "permission", whose payload is
//!   the whole map (`{ camera: "granted", microphone: "prompt" }`), emitted
//!   once after the plugin chain, on every ore change, and by a test's
//!   override. A status listens on it while it has a listener of its own,
//!   and fires "change" when its name's state differs from the last one it
//!   saw. Like every bus listener, that holds the engine until removed.
//! - A change event is a plain `{ type, target, currentTarget }` object
//!   (flux has no Event or EventTarget), and only function listeners count.

use std::cell::RefCell;
use std::collections::HashMap;

use ore::permission::{Permission, PermissionState};
use rquickjs::context::EvalOptions;
use rquickjs::{Ctx, Function, JsLifetime, Object, Value};

use crate::plugins::events::{emit_sticky, register_listener};

const SOURCE: &str = include_str!("permissions.js");

/// The file name stack frames inside the implementation cite.
const FILE_NAME: &str = "flux:permissions";

/// The sticky event the map travels on.
const EVENT: &str = "permission";

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
/// event, and every listening `PermissionStatus`, to see it.
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
pub fn emit(ctx: &Ctx<'_>) {
  let payload = Object::new(ctx.clone()).expect("create permission object");
  for (name, state) in snapshot(ctx) {
    payload.set(name, state).expect("set permission state");
  }
  emit_sticky(ctx, EVENT, payload);
}

/// Define `navigator.permissions`. Runs after the navigator is installed.
pub(crate) fn init_permissions(ctx: &Ctx<'_>) {
  let mut options = EvalOptions::default();
  options.filename = Some(FILE_NAME.to_string());
  let build: Function = ctx.eval_with_options(SOURCE, options).expect("evaluate permissions.js");
  let state_of = Function::new(ctx.clone(), state_of).expect("create the permission state hook");
  let listen = Function::new(ctx.clone(), listen).expect("create the permission listen hook");
  let permissions: Object = build.call((state_of, listen)).expect("build navigator.permissions");
  let navigator: Object = ctx.globals().get("navigator").expect("navigator global");
  navigator.set("permissions", permissions).expect("set navigator.permissions");
}

/// The hook behind `query` and `state`: the state name of the permission
/// named `name` in this engine, or None (undefined) for a name the query
/// refuses (not a permission, or declared away by the project).
fn state_of(ctx: Ctx<'_>, name: Value<'_>) -> rquickjs::Result<Option<&'static str>> {
  let Some(name) = name.as_string().map(|s| s.to_string()).transpose()? else {
    return Ok(None);
  };
  let Some(permission) = permission_named(&name) else {
    return Ok(None);
  };
  if crate::capability_excluded(&ctx, permission.name()) {
    return Ok(None);
  }
  Ok(Some(of(&ctx, permission).name()))
}

/// The hook a status listens through: `callback` on every `permission`
/// event, until the returned function is called.
fn listen<'js>(ctx: Ctx<'js>, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  register_listener(&ctx, EVENT.to_string(), callback, false)
}
