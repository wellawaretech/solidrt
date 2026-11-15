mod common;

use common::LogSink;
use flux::permission::{emit, set_override};
use flux::rquickjs::{Ctx, Function, Value};
use flux::FluxEngine;
use ore::permission::{Permission, PermissionState};

// The permission states as an embedder shapes them: a plugin's override is
// what a status's `state` reads and what the sticky `permission` event
// carries, and a later override plus `emit` fires a listening status's
// `change` once.

/// An engine whose plugin reports the camera denied, with
/// `stickyPermission()` returning the cached payload of the sticky
/// `permission` event (bare flux has no `on`) and `grantCamera()` setting
/// the camera granted and emitting.
fn engine(sink: &LogSink) -> FluxEngine {
  FluxEngine::builder()
    .logger(sink.logger())
    .plugin(|ctx| set_override(&ctx, Permission::Camera, PermissionState::Denied))
    .plugin(|ctx: Ctx<'_>| {
      let sticky = Function::new(ctx.clone(), sticky_permission).expect("create stickyPermission");
      ctx.globals().set("stickyPermission", sticky).expect("set stickyPermission");
      let grant = Function::new(ctx.clone(), grant_camera).expect("create grantCamera");
      ctx.globals().set("grantCamera", grant).expect("set grantCamera");
    })
    .build()
}

/// The cached payload of the sticky `permission` event.
fn sticky_permission<'js>(ctx: Ctx<'js>) -> Option<Value<'js>> {
  flux::sticky_cached(&ctx, "permission")
}

/// Report the camera granted in this engine, and emit the map.
fn grant_camera(ctx: Ctx<'_>) {
  set_override(&ctx, Permission::Camera, PermissionState::Granted);
  emit(&ctx);
}

#[tokio::test]
async fn an_override_reaches_the_status_the_payload_and_the_change_listener() {
  let sink = LogSink::new();
  engine(&sink)
    .eval_source(
      r#"
      let status = await navigator.permissions.query({ name: "camera" })
      console.log(status.state)
      let payload = stickyPermission()
      console.log(payload.camera)
      console.log(Object.keys(payload).join(","))
      let changes = 0
      let listener = () => { changes += 1 }
      status.addEventListener("change", listener)
      grantCamera()
      grantCamera()
      status.removeEventListener("change", listener)
      console.log(String(changes))
      console.log(status.state)
      "#,
    )
    .await;
  let out = sink.captured();
  let lines = out.lines_at(flux::LogLevel::Log);
  assert_eq!(lines, ["denied", "denied", "camera,microphone", "1", "granted"], "log: {lines:?}");
}
