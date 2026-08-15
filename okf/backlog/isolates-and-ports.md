---
title: Isolates and ports (compute off the JS thread)
description: A synchronous native call through flux:ffi or flux:wasm stalls the whole runtime (dropped frames in a GUI app, unanswered requests in a flux:http server) and there is no worker or thread primitive to move it to; build spawn + ports per okf/notes/channels-concurrency.md, with forge::Value as the neutral message type.
created: 2026-08-15
---

# Isolates and ports (compute off the JS thread)

## Problem

`flux:ffi` `symbols.*` calls and `flux:wasm` exports run synchronously on
the JS thread. A long call (decoding a multi-megabyte input, a heavy wasm
routine) has nowhere else to go: a GUI app drops frames, a `flux:http`
server stops answering for the duration. The incentive is inverted:
`flux:subprocess` is Promise-based, so shelling out is non-blocking while
the faster in-process route freezes the loop. This decides whether server
workloads that call native code are viable at all, and the same stall hits
plain heavy JS (large JSON parses, simulation ticks).

## Design source

okf/notes/channels-concurrency.md already worked this through: not CSP
channels, not the web `Worker` shape, but **spawn + ports** (Dart isolates,
actor mailboxes). Each spawned QuickJS runtime runs on its own thread with
its own heap and an inbox; messages are neutral values copied across,
shared-nothing. That note held implementation "until a concrete consumer
shows up"; blocking native calls are that consumer.

Why this and not a per-symbol async variant of `flux:ffi`
(`symbols.foo.async(...)` on a pool thread), which was the first shape
considered and is now dropped:

- Buffer arguments (okf/done/ffi-buffer-args.md) rely on the call being
  synchronous on the thread that owns the JS buffer. An off-thread call
  cannot pin it, so every async call would fall back to the malloc /
  writeMemory / readMemory dance that item removed. Inside an isolate the
  sync call plus buffer args keep working unchanged.
- Many C libraries are not thread-safe or keep per-thread state (errno,
  handles, global locks). A pool that runs any call on any thread invites
  races; an isolate is one thread that owns the library handle for its
  lifetime, serialising calls by construction. Parallelism is N isolates,
  each with its own handle.
- The same mechanism unblocks `flux:wasm` and heavy JS, which per-symbol
  async does nothing for.

## Done looks like

- `spawn(module) -> port`: run a JS module in a second flux runtime on its
  own thread. The isolate gets the non-gui `flux:*` modules (ffi, wasm,
  file, sqlite, http, ...) and its own event loop; the gui plugin is
  main-thread only and absent there.
- Ports: `send(value)`, awaitable `recv()`, close/terminate, and error
  propagation (an uncaught error in the isolate surfaces on the parent's
  port, not silently). Copy semantics; `ArrayBuffer` transfer for large
  payloads so a result buffer does not get copied twice.
- Isolate lifetime is structured: terminating the parent terminates its
  isolates (no leaked background runtimes; the note's nursery point).
- Recorded pitfalls hold: `Promise.race` over `recv()` is not `select`
  (losing branches leak messages), and no queue semantics for state-like
  data (that is a signal).

## Stages

1. **`forge::Value`** (DONE): engine-free neutral value plus JS marshalling
   both ways in flux. Decisions taken:
   - The set is `Null, Bool, Int(i64), Float(f64), String, Bytes, List,
     Map` (ordered pairs), i.e. the CBOR/msgpack vocabulary rather than the
     pure JS one, so SQLite values and JSON are strict subsets. JS numbers
     that are integral and within the safe range decode as `Int`.
   - forge result types describe themselves as a `Value` (`impl From<T> for
     Value` in forge: fs stat/dir entries, sqlite rows/results, subprocess
     output/status, mdns instances/hosts, p2p connInfo) and flux marshals `Value` in one
     place (`plugins/value.rs`, `Neutral` newtype). That replaced the
     per-type `IntoJs` impls those plugins carried; wasm/ffi coercion
     (signature-driven) and streaming byte chunks stay as they are.
   - JS -> Value contract (documented in `plugins/value.rs`): `undefined`
     -> `Null`, buffers and typed-array views -> `Bytes`, plain objects only,
     everything else a `TypeError`, depth cap instead of cycle detection.
2. **spawn + ports, minimal**: one isolate per `spawn`, one port pair,
   send/recv/terminate, error propagation, `ArrayBuffer` transfer. Module
   addressing: a bundled module path in dev; how a packed app carries an
   isolate module is part of this stage.
3. **Ergonomics**: typed ports, request/response helper, and only if
   experience demands it a native multi-recv (`select`).

Naming and exact API are decided at stage 2; the note's
`spawn(scriptOrModule, port)` is the starting point, and the SolidRT lens
applies (standard vocabulary where a standard exists, simplified
semantics, documented plainly). Types + docs in flux-types mirror it.

## Deliberately out of scope

`SharedArrayBuffer`/`Atomics`, shared-memory structs, and any per-symbol
async on `flux:ffi`.
