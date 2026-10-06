---
title: Remove the flux:ffi module
description: flux:ffi is the only unsandboxed app-facing surface, needs a prebuilt shared library for all seven targets against the one-payload rule, cannot byte-load on Android at all, and has no consumer in the tree, so once wasm modules compile to native on the device it has no remaining job that a custom runtime build does not do better.
created: 2026-10-03
completed: 2026-10-03
---

# Remove the flux:ffi module

Where the two jobs it covered go instead, front to back:
[design/wasm](../design/wasm.md).

## Symptom

`flux:ffi` exists as the escape hatch for app code that needs more than the
JavaScript interpreter, and nothing uses it. The tree holds the
implementation (`forge/src/ffi.rs`, `flux/src/forge_plugins/ffi.rs`), its
types, two `done/` records, one unstarted backlog item, docs listings and one
example (`flux/examples/ffi_buffer.js`). No package, no example app, no test.
The only known use was an out-of-tree experiment that left no trace in the
repo.

What keeping it costs:

- **It is the only unsandboxed app-facing surface.** Its own doc comment says
  it: no sandbox, full process rights, a signature that does not match the
  real ABI is undefined behavior, only load trusted code. Everything else in
  the API is safe by construction.
- **It needs a prebuilt shared library per target** (seven: linux x64 and
  arm64, darwin arm64, win32 x64, three Android ABIs), permanently at odds
  with the rule that an app's payload is the same bytes on every platform.
- **It does not work where it matters most.** Byte-loading is impossible on
  Android by OS policy and the APK packaging route is unstarted
  (the APK packaging backlog item, deleted with this), so the dev
  client can never run app-supplied native code on a device.
- **It carries `libffi = "=5.2.0"`**, vendored and built from source for every
  target because Android NDK and Windows have no system copy. `libloading`
  stays either way, since `lattice/src/gl_libs.rs` uses it to preload ANGLE.

`packages/core/agents/performance.md` already describes it as a binding tool
and warns that a synchronous call through it freezes rendering and input for
its duration.

## Shape

Delete the module and its dependency, and redirect the two jobs it covered:

- **Portable native-speed compute** goes to `flux:wasm`, sandboxed, one
  artifact from the author, compiled on the device
  ([wasm-native-execution](../backlog/wasm-native-execution.md)). The canonical demo for
  a module like this is a C program with a framebuffer, which is the shape
  wasm serves well: input in as scalars, frames out through linear memory
  straight into `uploadTexture` with no copy, and `emcc
  -sSTANDALONE_WASM=1 --no-entry` already produces a compatible module. The
  lua64 cartridge is the in-tree proof of the pattern.
- **Binding an existing native library, a platform SDK or a GPU API** goes to
  a custom runtime build, which is where anything touching engine state
  already had to live.

The condition on the removal: **the custom-build route has to be written down
in the same change.** It is not a product today (no documented flow, no
template, no third-party module registration story), and removing the hatch
without naming the replacement leaves "I need to call libfoo" with no
sanctioned answer. That is a docs task, not an engineering one.

## Done looks like

- `flux:ffi` gone from `flux/src/plugins/mod.rs`, `forge/src/ffi.rs`,
  `flux/src/forge_plugins/ffi.rs`, `packages/flux-types/modules/ffi.d.ts`, the
  module listings in `packages/flux-types/README.md` and
  `docs/runtime/index.md`, and `flux/examples/ffi_buffer.js`.
- `libffi` out of `forge/Cargo.toml`, with the size delta measured. It is a
  vendored C build on every target, so the win is a build-time one as much as
  a size one.
- The custom-build route documented as the answer for reaching a native
  library, and the changelog entry says what to use instead rather than only
  what was removed.
- The APK packaging item deleted rather
  than moved to `done/`: it describes work on a module that no longer exists.
- [physics-core](../backlog/physics-core.md) updated, since it names `flux:ffi` on
  desktop as the route for learning the API shape in a real game. That becomes
  a wasm module, which the instance stream mirror serves better anyway.

## Ordering

After the Cranelift flow, not before. Until a wasm module can run compiled,
removing the only native-speed route would leave a real gap rather than a
redirected one. Nothing else blocks it, because there is no consumer to
strand.

## Not affected

Four things share the name and stay:

- `forge/src/ktx2/ffi.rs`, `forge/src/video/vpx/ffi.rs` and
  `forge/src/video/opus/ffi.rs` are the project's own bindings to native
  libraries, which the dependency policy prefers over unproven wrapper crates.
- [ffi-write-batching](../backlog/ffi-write-batching.md) and
  [ffi-crossing-costs](../notes/ffi-crossing-costs.md) are about the
  rendertree plugin boundary (JS-to-Rust property writes), not this module.

## Outcome (2026-10-03)

Removed as listed under "Done looks like", with two differences from the
shaping above:

- **The ordering gate was dropped.** "After the Cranelift flow" was a speed
  argument: without on-device compilation there is no native-speed route.
  The reason to remove the module is distribution, not speed: an app payload
  that carries a shared library is per platform whatever the speed of the
  alternatives, and there was no consumer to strand. So it went first, and
  the wasm engine work stands on its own gate.
- **The custom-build route is documented as it is, not as it should be.**
  `docs/runtime/index.md` ("Native code") names the two routes. The
  depend-not-fork shape of a custom build (a cargo project over the lattice
  crate with extra modules registered, no fork) needs a registration hook
  on lattice's entry points that does not exist yet:
  [runtime-extension-modules](../plans/runtime-extension-modules.md).

Why not a runtime extension mechanism in ffi's place (a native library the
runtime dlopens, with a C ABI to register a module): on Android the only
native code that may run is what arrived inside the APK, and iOS forbids
loading downloaded code outright, so a native extension there is a custom
build by definition. The platforms that matter most collapse "default
runtime plus extension" into "custom runtime", and a C ABI over the engine
would be a second, large surface to keep stable for the desktop-only case.

Also removed: the `libffi` row in `scripts/build-third-party-notices.ts`
and the vendored `libffi-sys` build from every target. The size delta is
not measured yet (no release-opt build in this session); one line in
tiny.md under Runtime.

Release note, at the next tag: under Breaking changes, `flux:ffi` removed;
use `flux:wasm` for portable compute or a custom runtime build for a native
library, with the "Native code" section as the pointer.
