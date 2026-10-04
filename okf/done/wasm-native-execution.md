---
title: Compile wasm to native on the device
description: flux:wasm runs on an interpreter, so it is a portability tool and not a speed tool; put wasmtime with Cranelift behind the forge API wherever the device allows native code and compile the shipped wasm on the device at install, with wasmi (SIMD on) as the one engine of the iOS and armeabi-v7a builds, at a decided +7.8 MB of runtime on the native targets.
created: 2026-10-03
completed: 2026-10-04
---

# Compile wasm to native on the device

The full model this belongs to, front to back, is
[design/wasm](../design/wasm.md). This item is the work.

## Symptom

An app that needs more than the JavaScript interpreter for one module has no
fast route. `flux:wasm` runs on wasmi, a pure interpreter, so tight compute
runs a small constant factor over QuickJS at best and call-heavy code can be
slower. The alternatives either ship per-platform binaries against the
one-payload rule ([ffi-module-removal](../done/ffi-module-removal.md)) or
require a custom runtime build.

## Shape

Two engines behind the one `forge::wasm` API, one per build, decided
2026-10-04: wasmtime with Cranelift wherever a native backend exists and
runtime codegen is allowed (desktop, android-arm64), wasmi with SIMD
everywhere else (iOS, armeabi-v7a). The cargo feature selects which one a
build carries, never both. Pulley as the interpreted lane was measured and rejected
([wasm-interpreter-throughput](../notes/wasm-interpreter-throughput.md)): it
would put the compiler and an install-time compile on the devices that gain
nothing from them, and it is not faster than wasmi there. The two engines are
held to one proposal set, wasmi's, so a module that validates on one lane
validates on all.

Compilation happens on the device, at install, into a cache keyed by module
hash plus the engine's compatibility hash. The payload stays one portable
`.solapp` and pack never compiles. `flux:wasm` does not change shape; whether
a module arrives precompiled is hidden in the loader.

## The cost

Measured and accepted: **+7.8 MB uncompressed** (41.90 to 49.73 MB on the
`solidrt` runtime at `release-opt`; wasmtime 48.0.5 with
`default-features = false` and the features `runtime`, `std`, `pulley`,
`cranelift`; the build ships `wat` instead of `pulley`, both small), +1.93 MB
compressed with zstd-19, and essentially nothing reclaimed by LTO or by
sharing. That is the native builds' whole cost, since they carry no wasmi;
the interpreter builds grow by wasmi's SIMD tables, 0.61 MB, instead. Table and method in the design document's
"Measured cost". The size is the price of running fast and is not reopened
here; the build knob below is the escape hatch for a build that needs the
bytes back.

Winch, wasmtime's baseline compiler, is rejected: it trades code quality for
compile speed, which is the wrong trade when a module is compiled once at
install and run on every frame.

## Done looks like

- `flux:wasm` unchanged in shape, with modules running compiled wherever the
  device allows and interpreted where it does not.
- `Flux.capabilities` reports `wasm-native` when the active engine is
  wasmtime. Computed from the engine configuration at context setup, not a
  static list entry, so the dev switch below flips it.
- The interpreted lane testable on a desktop with the engine the small
  targets really run: a `WASM_NATIVE=0` build of the dev client, which is
  wasmi alone. Without that nobody experiences the iOS or armeabi-v7a path,
  which is the same class of mistake as quoting timings from a debug build.
  A runtime switch was built and taken out again: it needed wasmi in every
  native build, a dev convenience paid for in every shipping runtime.
- A visible answer for the first compile: the overlay badge reads
  `INSTALLING` (`Badge::Installing` in `lattice/src/overlay.rs`, next to
  `Connected` and `Muted`) for the whole of a pushed version's install, which
  is where the pre-warm runs. This covers the dev-push path now. The packed and OTA
  path gets the same answer when stage 4 of
  [client-storage-updates](../plans/client-storage-updates.md) seeds the
  factory version into the store, since install is the one hook; until then
  the packed runner compiles on the first `new Module`.
- The `wasm.d.ts` note rewritten: "runs in a pure interpreter (wasmi, no JIT),
  so this is a portability tool, not a speed tool" becomes wrong, and that
  sentence shapes expectations more than anything else here.
- Throughput numbers for QuickJS against wasmi and Cranelift-native, which
  closes [wasm-vs-js-throughput](../done/wasm-vs-js-throughput.md): measured in
  [wasm-vs-js-throughput](../notes/wasm-vs-js-throughput.md).

## Decisions

- **Build knob.** `wasm-native` cargo feature on forge (wasmtime with
  Cranelift), passed through flux and lattice; `WASM_NATIVE` in
  `lattice/Makefile` on the `VIDEO` pattern, default on for the desktop and
  android-arm64 goals and off for armeabi-v7a. `WASM_NATIVE=0` is today's
  wasmi-only build. With the feature on, the wasmi module is cfg'd out; the
  crate stays a dependency so the manifest needs no second feature, and the
  linker drops it unreferenced.
- **Two engines, one API, one per build.** `forge/src/wasm/` with the facade
  in `mod.rs` and `interp.rs` (wasmi) or `native.rs` (wasmtime) selected by
  cfg; the public types, method set and error wording stay as they are. The
  one test file runs on whichever engine the build has, so CI runs forge
  twice. wasmtime is configured to wasmi's proposal set (SIMD and relaxed
  SIMD on; exceptions, GC, threads, stack switching off).
- **Engine ownership.** A `WasmEngine` is a cheap clone of a shared handle.
  The embedder makes one per process and hands it to the flux engine builder
  (`wasm_engine`, carried in `EngineConfig` next to `cache_dir`, read by the
  plugin as fetch reads its cache dir) and to the store's install, so a
  pre-warm fills exactly the slots the app engine reads. A builder given none
  makes a private one. No process global.
- **How a module is found: by content.** The cache key is the sha256 the
  manifest already stores per asset plus wasmtime's compatibility hash.
  `new Module(bytes)` hashes the bytes and hits the slot the install pre-warm
  filled, so nothing is declared and nothing can drift. The pre-warm compiles
  every `.wasm` asset of the installed version, which is the "every `.wasm`
  under `assets/` by convention" option with no list to keep in sync. A
  module from any other source (fetched, inline wat) compiles on first use
  and is cached the same way.
- **Where the cache lives.** `<app cache dir>/wasm/<engine hash>/<module
  sha256>.cwasm`, next to the fetch cache, so it leaves with the app and
  never crosses apps. The engine hash digests wasmtime's compatibility hash
  (version, target, tunables, features) through sha256, so an artifact is
  only ever offered to the configuration that produced it. Like every disk
  cache it is the embedder's opt-in through the engine builder's
  `cache_dir`, read by the plugin the way fetch reads it: bare `flux` and
  `fluxrt` compile in memory and never write. Artifacts are written beside
  their name and renamed into place.
- **Retention.** Keep what a held version's manifest references, prune the
  rest at install alongside the version prune, and drop whole directories of
  a stale engine hash. Held means every version dir the store's own prune
  leaves standing: measured 2026-10-04, an artifact is 2.5x (Pulley) to 3.0x
  (x86_64) the wasm, and artifacts dedupe by hash across versions, so the
  cache is bounded by three times the wasm the store itself holds and needs
  no retention policy of its own.
- **Fail-soft.** A cache that cannot be read or written means compiling in
  memory each launch. A compile failure on a valid module is an error, as a
  parse failure is today; with one engine there is no interpreter to fall
  back to.
- **One proposal set on every lane**, equal to what Pulley supports, so a
  module that validates on one device validates on all. SIMD on if Pulley has
  it.
- **Memory reservation.** wasmtime's 64-bit default on desktop and
  android-arm64. Pulley and 32-bit hosts run explicit bounds checks with a
  small reservation, verified on the TV.
- **Compile after verify, never before.** The pre-warm runs after
  `install_at` has checked every hash. The signed manifest is what makes
  compiling OTA-delivered content sound, so the ordering is a requirement and
  not an implementation detail. The trust boundary in
  [update-mechanism](../notes/update-mechanism.md) must not blur.
- **Order of work.** The forge backend split and the wasmtime backend first,
  with the tests green on both; then the cache and the pre-warm; then
  capability, badge, knob and wording; then the JavaScript throughput
  run. An armeabi-v7a client build along the way confirms the wasmi-only
  configuration still compiles, since the split touches it.

## Involves

`forge/src/wasm.rs` becomes `forge/src/wasm/` with the facade and two
backends: 498 lines written against wasmi's API plus 233 of tests. The
wasmtime host-call path is the real work. wasmi suspends the frame and hands
the store back, which is why a host handler may re-enter the instance freely;
wasmtime holds the store across a host call and hands the closure a
`Caller`, so re-entrant calls route through the innermost active caller
behind one helper. The caller and the in-flight handler are raw pointers
published through a guard that restores the previous value on drop, so an
unwinding panic in a handler cannot leave a dangling pointer: two documented
unsafe blocks, and a test that panics through a handler and calls again. The
existing tests (`reentrant_host_handler`, `unwind_through_nested_activation`
above all) are the contract both backends must pass. The public types and
error wording stay.

`flux/src/forge_plugins/wasm.rs` unchanged in surface; the engine builder and
context setup for the `WasmEngine` and the capability; `lattice/src/go/store.rs`
for the pre-warm and retention; `lattice/src/overlay.rs` for the badge;
`lattice/Makefile` for the knob; `packages/flux-types/modules/wasm.d.ts` for
the contract wording.

Platform notes. Android arm64: the compiled artifact is file-mapped; API 29
and up block `execve` of app-data files, not executable mappings (`dlopen`
from the data dir works). Verified 2026-10-04 on the arm64 tablet (Android
12): a pushed version's `.wasm` asset was compiled at install in 5 ms, the
app's `new Module` loaded the 198 KB artifact from the app cache in 0.5 ms
with no recompile, and a 2M-iteration loop ran in 8.3 ms with
`wasm-native` reported. The armeabi-v7a TV (Android 8.0) ran the same
module on the interpreter-only build the same day: lane reported as
interpreter, parse 26.8 ms, the loop 436 ms, same result, no pre-warm and no
cache directory written. iOS runs wasmi, so no wasmtime question arises for
the iOS port.

Consumers that would use it: [physics-core](../backlog/physics-core.md).
