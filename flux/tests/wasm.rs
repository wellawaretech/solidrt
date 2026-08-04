#![cfg(feature = "compile")]

mod common;

use common::run_source;

// JS-surface tests for flux:wasm, focused on what the plugin adds over the
// forge core (which has its own tests): the `instance.memory` ArrayBuffer
// view with its detach-on-grow contract, and the JS-visible error messages.
// Modules are embedded as wat text (forge enables wasmi's `wat` feature).

#[tokio::test]
async fn memory_view_aliases_linear_memory() {
  let out = run_source(
    r#"
            import { Module } from "flux:wasm";
            let mod = new Module(new TextEncoder().encode(`(module
              (memory (export "memory") 1)
              (func (export "peek") (param i32) (result i32) local.get 0 i32.load8_u))`));
            let instance = mod.instantiate({});
            let mem = instance.memory;
            console.log(mem.byteLength);
            instance.writeMemory(64, new Uint8Array([1, 2, 3]));
            let view = new Uint8Array(mem);
            console.log(`${view[64]} ${view[65]} ${view[66]}`);
            view[65] = 42;
            console.log(instance.call("peek", 65));
            console.log(instance.memory === mem);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "65536\n1 2 3\n42\ntrue");
}

#[tokio::test]
async fn overlapping_write_memory_is_staged() {
  let out = run_source(
    r#"
            import { Module } from "flux:wasm";
            let mod = new Module(new TextEncoder().encode(`(module
              (memory (export "memory") 1))`));
            let instance = mod.instantiate({});
            instance.writeMemory(64, new Uint8Array([1, 2, 3]));
            // The source is a view over this instance's own memory; the copy
            // must be staged, not aliased.
            instance.writeMemory(65, new Uint8Array(instance.memory, 64, 2));
            let view = new Uint8Array(instance.memory);
            console.log(`${view[64]} ${view[65]} ${view[66]}`);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "1 1 2");
}

#[tokio::test]
async fn growth_detaches_the_view() {
  let out = run_source(
    r#"
            import { Module } from "flux:wasm";
            let mod = new Module(new TextEncoder().encode(`(module
              (memory (export "memory") 1 4)
              (func (export "grow") (result i32) i32.const 1 memory.grow))`));
            let instance = mod.instantiate({});
            let before = instance.memory;
            instance.call("grow");
            try {
              new Uint8Array(before);
              console.log("stale view alive");
            } catch (e) {
              console.log("detached");
            }
            console.log(instance.memory.byteLength);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "detached\n131072");
}

#[tokio::test]
async fn mid_call_growth_detaches_before_host_dispatch() {
  let out = run_source(
    r#"
            import { Module } from "flux:wasm";
            let mod = new Module(new TextEncoder().encode(`(module
              (import "env" "check" (func $check))
              (memory (export "memory") 1 4)
              (func (export "run") i32.const 1 memory.grow drop call $check))`));
            let before;
            let instance = mod.instantiate({
              env: {
                check: () => {
                  try {
                    new Uint8Array(before);
                    console.log("stale view alive");
                  } catch (e) {
                    console.log("detached before handler");
                  }
                },
              },
            });
            before = instance.memory;
            instance.call("run");
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "detached before handler");
}

#[tokio::test]
async fn memory_undefined_without_export() {
  let out = run_source(
    r#"
            import { Module } from "flux:wasm";
            let mod = new Module(new TextEncoder().encode(`(module (func (export "noop")))`));
            let instance = mod.instantiate({});
            console.log(instance.memory === undefined);
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true");
}

#[tokio::test]
async fn call_errors_name_target_and_signature() {
  let out = run_source(
    r#"
            import { Module } from "flux:wasm";
            let mod = new Module(new TextEncoder().encode(`(module
              (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add)
              (table 1 funcref)
              (func $wrong (param f64))
              (elem (i32.const 0) $wrong)
              (type $expected (func (param i32)))
              (func (export "bad") i32.const 7 i32.const 0 call_indirect (type $expected)))`));
            let instance = mod.instantiate({});
            try { instance.call("add", 1); } catch (e) { console.log(e.message); }
            try { instance.call("add", 1, "x"); } catch (e) { console.log(e.message); }
            try { instance.call("bad"); } catch (e) {
              console.log(`${e.message.includes("wasm call to bad failed")} ${e.message.includes("stale function pointer")}`);
            }
            "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(
    out.log(),
    "add (i32, i32) -> i32 expects 2 argument(s), got 1\n\
     add (i32, i32) -> i32: argument 1: expected a number (i32)\n\
     true true"
  );
}
