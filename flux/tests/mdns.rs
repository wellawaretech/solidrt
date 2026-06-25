#![cfg(feature = "compile")]

mod common;

use common::run_source;
use std::time::Duration;

// flux:mdns exercised through real JS. Full end-to-end resolution of `.local`
// names needs a LAN with a Bonjour/avahi responder, which the sandbox lacks, so
// these assert the deterministic no-responder paths: the surface exists, the
// queries resolve to arrays (empty here), and nothing crashes. Each run is wrapped
// in a hard timeout so a hung socket op fails cleanly instead of blocking the
// suite. The query timeoutMs is kept short so the no-answer window closes fast.
async fn run(code: &str) -> common::Captured {
  tokio::time::timeout(Duration::from_secs(10), run_source(code)).await.expect("flux:mdns test timed out")
}

#[tokio::test]
async fn resolve_empty_input_is_empty() {
  // No addresses to resolve: resolves to [] without touching the network.
  let out = run(
    r#"
        import { resolve } from "flux:mdns"
        let r = await resolve([])
        console.log(Array.isArray(r), r.length)
        "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true 0");
}

#[tokio::test]
async fn browse_unknown_service_resolves_to_array() {
  // A service nobody advertises yields an empty array within the timeout window.
  let out = run(
    r#"
        import { browse } from "flux:mdns"
        let r = await browse("_nonexistent._tcp", { timeoutMs: 300 })
        console.log(Array.isArray(r))
        "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true");
}

#[tokio::test]
async fn services_resolves_to_array() {
  // The service-enumeration query resolves to an array (possibly empty here).
  let out = run(
    r#"
        import { services } from "flux:mdns"
        let r = await services({ timeoutMs: 300 })
        console.log(Array.isArray(r))
        "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true");
}