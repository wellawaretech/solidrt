---
title: Isolates (compute off the JS thread)
description: A synchronous native call through flux:ffi or flux:wasm stalls the whole runtime (dropped frames in a GUI app, unanswered requests in a flux:http server) and there is no worker or thread primitive to move it to; the mechanism is a second flux runtime per "use isolate" module, addressed by ordinary typed function calls, with forge::Value as the neutral message type.
created: 2026-08-15
---

# Isolates (compute off the JS thread)

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
channels, not the web `Worker` shape, but shared-nothing isolates: each
spawned QuickJS runtime runs on its own thread with its own heap; values are
neutral `forge::Value`s copied across. That note held implementation "until
a concrete consumer shows up"; blocking native calls are that consumer.

Why isolates and not a per-symbol async variant of `flux:ffi`
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

## The shape: a module is an isolate, calls are the protocol

The first cut (`spawn(sourceText) -> Port`, message loops written by hand)
put source code in strings and made the child's message vocabulary the
app's problem. Rejected: runtime compilation of strings is not acceptable,
and the "seam" (what a message means) is exactly what a runtime should own.
The current design mirrors what RSC/SolidStart do with `"use server"`:

```ts
// worker.ts
"use isolate"
import { open } from "flux:ffi"
let lib = open("libfoo.so", {...})            // module state lives in the isolate

export function decode(buf: Uint8Array) { return lib.symbols.decode(buf) }
export function sum(n: number) { let s = 0; for (let i = 0; i < n; i++) s += i; return s }
```

```ts
// main.ts
import { isolate } from "flux:isolate"
import type * as Worker from "./worker"

let worker = isolate<typeof Worker>("worker")   // one instance, spawned lazily
let s = await worker.sum(1_000_000)             // runs on the isolate's thread
```

- The `"use isolate"` directive marks a module as an isolate entry. It is
  inert syntax; `srt` sees it, bundles the module and its imports as their
  own bundle (id = path relative to the source root without extension), and
  a `"use isolate"` module imported by value from the main bundle is a build
  error (`import type` is the sanctioned form). Shared helper modules end up
  in both bundles as independent copies, like RSC's shared modules.
- `isolate<T>(id, opts?)` returns a `Proxy`: any property is
  `(...args) => Promise`. The first call spawns one child runtime for `id`;
  calls are `{id, fn, args}` messages, executed in order by a native
  dispatcher in the child that looks up the export by name; the return value
  (or thrown error) resolves (rejects) the promise. `terminate()` kills the
  instance. Each `isolate()` call is its own instance, so more instances of a
  module is more calls; no pool.
- Where the module comes from is the embedder's business: an
  `EngineConfig` hook `isolate_resolver(id) -> source | bytecode`, inherited
  by children so isolates nest. Standalone `flux app.js` resolves
  `<entry dir>/<id>.js`; lattice resolves through the app's assets (below).
- The child never sees a port: `flux:isolate` in the child is the same
  module (it can spawn isolates of its own).
- Types: `Isolated<T>` maps each function export to its Promise-returning
  form and leaves async generators as they are.

Compared with the actor/inbox form: the actor is the mechanism underneath
(spawn a runtime, a bidirectional `Link` of messages, a kill switch), the
call model is a generated inbox loop with a fixed vocabulary. What a free
actor could additionally do (push unprompted, end itself, own a richer
protocol) is either expressible as calls plus async-generator exports (a
never-ending generator is a subscription the isolate pushes into) or is not
wanted (pushing to a main that has not subscribed).

## Done looks like

- `flux:isolate` exports `isolate<T>(id, opts?)`; a `"use isolate"` module
  in a SolidRT project is callable from main as above, in dev and packed.
  (Both hold as of stage 4.)
- Uncaught errors in the isolate surface on the caller: a failed module load
  or a throw rejects the pending calls with the error; an exited instance
  rejects later calls with a message that names the cause.
- Isolate lifetime is structured: terminating the parent terminates its
  isolates (no leaked background runtimes; the note's nursery point).
- Recorded pitfalls hold: no queue semantics for state-like data (that is a
  signal); values are copies.

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
2. **spawn + ports over source text** (DONE, then superseded by stage 3):
   `spawn(source, { args? }) -> Port`, `port` in the child. What survives
   as the substrate:
   - Engine-free half (`forge::isolate`: `Link` pair over tokio mpsc, `Kill`)
     separate from the flux thread spawn.
   - Uncaught errors in the child (module throw, unhandled rejection, throw
     out of a callback) are logged and forwarded on the link.
     `FluxEngineBuilder::on_uncaught` funnels every such site
     (`logger::report_error`). Fixing this exposed a pre-existing double
     report of a synchronous top-level throw (QuickJS rejects an internal
     promise nobody can observe); the entry-promise handler drops the
     duplicate from the rejection log.
   - `terminate()` = interrupt flag (`FluxEngineBuilder::interrupt_flag`,
     wired to `set_interrupt_handler`) + drop the child's engine future.
     Children die with the parent context via a userdata registry whose
     `Drop` fires every kill switch, so reload/exit is covered transitively.
   - The child is built from the parent's `EngineConfig` (logger, fetch
     cache dir, user agent, stack size; `FluxEngine::config(&ctx)` +
     `FluxEngineBuilder::from_config`), so host config is inherited by
     construction rather than re-derived.
   - `ArrayBuffer` transfer is NOT in: bytes are copied in and out. Zero-copy
     needs the buffer allocated with a flux-owned free hook; do it when a
     payload size makes it show up.
3. **Calls: runtime half (flux)** (DONE): `isolate<T>(id, opts?)` is a
   `Proxy` built natively (the `Proxy` constructor from globals with a
   native `get` trap; no JS glue is evaluated) over `forge::isolate::Msg`
   `Call`/`Reply`/`Error` on the link; the child's dispatcher
   (`FluxEngine::eval_module` hands the evaluated namespace to a native
   loop) looks exports up by name, awaits promises, answers in order.
   `EngineConfig::isolate_resolver` (inherited by children) supplies the
   module as `ModuleCode::Source | Bytecode`; standalone `flux` resolves
   `<entry dir>/<id>.js`. `spawn(source)`/`Port`/`port` are gone from the
   JS surface. Parent-side reply routing runs inside the call futures
   (whoever holds the link reads for everyone) rather than a spawned loop,
   because a `ctx.spawn` task never lets the runtime go idle. Types, docs,
   `flux/examples/isolate.js` + `isolate_worker.js`.
4. **Calls: toolchain half (srt + lattice)** (DONE): verified dev push,
   watcher and MCP reload, packed executable and pack folder. Decisions:
   - Detection is a scan of the entry's directory tree for files whose first
     statement is the directive (`findIsolateModules` in `bundler.ts`), not
     the main build: `import type` never loads the file. Each module is its
     own `Bun.build` (splitting stays off, so a helper both import is
     duplicated). Loading a directive module by value inside any build (main
     or another isolate) fails that build with a message naming the file.
   - Delivery rides the manifest asset rail unchanged: `isolates/<id>.js`
     (dev source) / `isolates/<id>.bin` (pack bytecode) are ordinary
     manifest `assets` entries. The rail was `assets/`-gated in three places
     (`lattice::manifest::safe_asset_path`, the `forge::fs` mount, the dev
     server's route) and now also admits `isolates/`; nothing else in the
     store, fetch or pack path knows about isolates.
   - Dev bundles are written to `<project>/.srt-data/isolates/` by srt
     (`bundle()`) and by the server-side rebuild; the dev server serves
     `/isolates/` from there and clients install them like any asset.
   - lattice's resolver (`resolve_isolate` in `lib.rs`, set on every engine
     build) reads `isolates/<id>.bin` then `.js` through the mount via
     `forge::fs::read_sync`, so the installed version dir, a pack folder and
     the packed image resolve alike; unmounted (no store) means no isolates.
   - `srt check`/the startup typecheck add every isolate module to `files`,
     so one nothing `import type`s is still checked. `srt bundle` and
     `srt pack --flux` do not carry isolates (no manifest to ride).
5. **Only on demand**: async-generator exports (yield/return/next/cancel
   messages; a never-ending generator is a subscription), `AbortSignal` on
   calls, `instances` on a handle.

## Deliberately out of scope

`SharedArrayBuffer`/`Atomics`, shared-memory structs, any per-symbol async
on `flux:ffi`, source-text or bytecode-blob spawning from JS.