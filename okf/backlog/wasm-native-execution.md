---
title: Compile wasm to native on the device
description: flux:wasm runs on an interpreter, so it is a portability tool and not a speed tool; swap wasmi for wasmtime and compile the shipped wasm to native code on the device at install, which costs a measured 7.8 MB of runtime and is gated on whether that is affordable.
created: 2026-10-03
---

# Compile wasm to native on the device

The full model this belongs to, front to back, is
[design/wasm](../design/wasm.md). This item is the work.

## Symptom

An app that needs more than the JavaScript interpreter for one module has no
fast route. `flux:wasm` runs on wasmi, a pure interpreter, so tight compute
runs a small constant factor over QuickJS at best and call-heavy code can be
slower. The alternatives either ship per-platform binaries against the
one-payload rule ([ffi-module-removal](ffi-module-removal.md)) or require a
custom runtime build.

## Shape

wasmtime in place of wasmi: one engine in two modes, Cranelift-compiled native
where there is a backend and Pulley where there is not. Compilation happens on
the device at install, into a cache keyed by module hash plus engine version,
fail-soft to interpretation. The payload stays one portable `.solapp` and pack
never compiles. `flux:wasm` does not change shape; whether a module arrives
precompiled is hidden in the loader.

Sections 2, 3 and 5 of the design document carry the reasoning, the per-target
lanes, the deliberate non-goals and the iOS case.

## The gate

Measured: **+7.8 MB uncompressed** (41.90 to 49.73 MB on the `solidrt`
runtime at `release-opt`), +1.93 MB compressed with zstd-19, and essentially
nothing is reclaimed by LTO or by sharing. Full table and method in the design
document's "Measured cost".

Whether that is affordable is the whole decision. It competes directly with
[runtime-optimization](runtime-optimization.md), and the only clean size-only
precedents in the tree are an order of magnitude smaller (1.28 MB VP9 encoder,
0.9 MB texture encoder).

## Done looks like

- `flux:wasm` unchanged in shape, with modules running compiled wherever the
  device allows and interpreted where it does not.
- A capability reporting which lane is active, so an app can adapt its own
  load instead of guessing.
- A dev switch that forces the interpreter, so the interpreted lane is
  testable. Without it nobody experiences the iOS or armeabi-v7a path, which
  is the same class of mistake as quoting timings from a debug build.
- A visible answer for the first-launch compile, on install and on update. It
  is a one-time cost per device per app by design, but a multi-second pass
  with nothing on screen reads as a hang, and it is the normal path for every
  app rather than an OTA edge case.
- The `wasm.d.ts` note rewritten: "runs in a pure interpreter (wasmi, no JIT),
  so this is a portability tool, not a speed tool" becomes wrong, and that
  sentence shapes expectations more than anything else here.

## Decisions to make first

- **Binary size gate.** The number above, then set the default the way
  `SPEECH` is set in `lattice/Makefile`. Cranelift is off where there is no
  backend regardless, so iOS and armeabi-v7a clients stay small for free.
  Measure Winch as a third option before deciding (design document, "Measured
  cost").
- **Pulley against wasmi.** The bar is "not meaningfully slower", not
  "faster": the win lives on the compiled targets and the interpreted lane
  only has to avoid regressing. Same harness as
  [wasm-vs-js-throughput](wasm-vs-js-throughput.md), which this would settle
  for three engines instead of two. If Pulley loses badly, keeping wasmi for
  the interpreted targets is the fallback, at the cost of the one-semantics
  property.
- **How a module is found.** Every `.wasm` under `assets/` by convention,
  matching how assets are already collected wholesale, or declared in the
  `solidrt` key of package.json. Convention avoids a list to keep in sync.
- **Retention.** Compiled artifacts are several times the size of the wasm and
  the version store keeps five versions per app across multiple apps. Measure
  before settling the policy.
- **Memory reservation.** wasmtime reserves a large virtual region per linear
  memory so bounds checks compile away. Free on 64-bit desktop, a config pass
  for mobile, and a 32-bit target needs dynamic memories with explicit checks.
- **Compile after verify, never before.** The signed manifest is what makes
  compiling OTA-delivered content sound, so the ordering is a requirement and
  not an implementation detail. The trust boundary in
  [update-mechanism](../notes/update-mechanism.md) must not blur.

## Involves

`forge/src/wasm.rs` for the engine swap (498 lines written against wasmi's
API, plus 233 lines of tests, and the host-call path is resumable-call shaped
in wasmi and different in wasmtime, so this is a port and not a config
change), `flux/src/forge_plugins/wasm.rs` unchanged in surface, the version
store and app cache for the artifact cache, lattice for the capability and the
first-launch surface, and `packages/flux-types/modules/wasm.d.ts` for the
contract wording.

Consumers that would use it: [physics-core](physics-core.md).
