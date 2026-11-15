mod common;

use common::LogSink;
use flux::permission::{emit, set_override};
use flux::rquickjs::{Ctx, Function, Value};
use flux::FluxEngine;
use ore::permission::{Permission, PermissionState};

// The permission states as an embedder shapes them: a plugin's override is
// what `Flux.permission` reads and what the sticky `permission` event
// carries, and a later override plus `emit` replaces the cached payload.

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
async fn an_override_reaches_the_read_and_the_sticky_payload() {
  let sink = LogSink::new();
  engine(&sink)
    .eval_source(
      r#"
      console.log(Flux.permission("camera"))
      let payload = stickyPermission()
      console.log(payload.camera)
      console.log(Object.keys(payload).join(","))
      grantCamera()
      console.log(Flux.permission("camera"))
      console.log(stickyPermission().camera)
      "#,
    )
    .await;
  let out = sink.captured();
  let lines = out.lines_at(flux::LogLevel::Log);
  assert_eq!(lines, ["denied", "denied", "camera,microphone", "granted", "granted"], "log: {lines:?}");
}
