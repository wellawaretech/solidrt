---
title: Put every optional flux module behind a cargo feature
description: A custom runtime can only leave out what is a cargo feature all the way down, and today p2p, wasm, sqlite, subprocess, mdns and QR scanning are unconditional while video, ktx2 and the wasm lane are Makefile knobs; make each optional module a lattice feature, chosen in the runtime crate's Cargo.toml with the stock set as default and the player carrying all of them, starting with wasm.
created: 2026-10-06
---

# Put every optional flux module behind a cargo feature

## Symptom

The custom runtime route
([runtime-extension-modules](../done/runtime-extension-modules.md),
[custom-runtime-authoring](../done/custom-runtime-authoring.md)) builds
lattice as a crate, and it can leave out only what is a cargo feature at
every layer: the dependency, the forge (or alloy) module, the flux plugin
registration and the entry in `BASE_CAPABILITIES`. Today that holds for
`video`, `ktx2`, `wasm-native` and `test`, and even those are
chosen by Makefile knobs (`VIDEO=0`, `KTX2=0`, `WASM_NATIVE=0` in
`packages/cli/runtime/runtime.mk`) passed as `lattice/<feature>` on cargo's
command line, not in the runtime's manifest.

The rest are unconditional. `iroh` is an unconditional dependency in
`forge/Cargo.toml`, `flux:p2p` is registered unconditionally in
`flux/src/plugins/mod.rs`, and `"p2p"` is a fixed capability. The same is
true of `flux:wasm` (only the lane is a feature; wasmi is always compiled),
`flux:sqlite`, `flux:subprocess`, `flux:mdns`, and QR scanning (rxing in
`alloy/src/barcode.rs`, behind the camera's scan option). A custom runtime
that has no use for peer-to-peer gets iroh and its TLS and QUIC stack
anyway.

The size work in [runtime-optimization](runtime-optimization.md) names the
p2p stack and sqlite among the unmeasured weights; a feature per module is
also what makes those measurable, one build with and one without.

## Decided (2026-10-12)

- **The runtime crate's Cargo.toml chooses.** Lattice declares the
  features, with `default` the stock set (every optional module on). A
  custom runtime trims on its lattice dependency line, the cargo way:

  ```toml
  lattice = { git = "...", tag = "v...", default-features = false, features = ["video", "wasm"] }
  ```

  The scaffold keeps the plain line, so a runtime that says nothing gets
  the stock set. The capability knobs in runtime.mk go, and the runtime
  builds with no feature flags. Cargo cannot subtract one default, so a
  trimmed runtime lists everything it keeps, and a module lattice later
  adds to `default` does not reach it until its manifest is edited.
  Accepted: the alternative, negative features (`no-p2p`), breaks
  additivity.
- **The player carries everything.** Lattice's `go` turns on every
  optional module, whatever the runtime crate lists. That covers what the
  dev flow itself needs (iroh for the tunnel and connection under
  `lattice/src/go/`, QR scanning) and lets one player run any app. The
  price: dev no longer catches an import the trimmed runtime lacks, so that
  check moves to the build (below).
- **Features stay additive, and a per-target limit lives in the crate,**
  not in the Makefile. Wasm is the case: no feature, `wasm` (wasmi), or
  `wasm-native` (Cranelift where the target has a backend, wasmi
  elsewhere), each implying the one before.
- **`test` stays the Makefile's.** Whether the player has the test host is
  a build goal's choice, not an app's.
- **One module at a time, wasm first,** to see how the mechanism works
  before the rest follow.

## Shape

The `video` pattern, per module:

- **forge** owns the dependency: `p2p = ["dep:iroh"]`, with `pub mod p2p`
  and the iroh-only dependencies behind `cfg(feature = "p2p")`. A feature
  that gates the module but not the dependency shrinks nothing. QR
  scanning is the exception: alloy owns rxing (`barcode = ["dep:rxing"]`).
- **flux** passes it through: `p2p = ["forge/p2p"]`, gating the plugin
  module, the `add_module` lines and the capability entry, so
  `Flux.capabilities` tells the truth for a build without it. The flux
  binary (`flux/Makefile`, what `sol` runs) keeps every module on.
- **lattice** passes it through (`p2p = ["flux/p2p"]`) and lists it in
  both `default` and `go`.

The candidates, by what they pull in:

| module | dependency the feature would own |
|---|---|
| `flux:wasm` | wasmi, and wasmtime under `wasm-native` |
| `flux:p2p` | iroh and its network stack |
| `flux:sqlite` | the sqlite binding |
| `flux:subprocess` | the process core |
| `flux:mdns` | the DNS codec (shared with iroh's deps while p2p is on; `forge/src/net.rs` also uses hickory or netdev, check which) |
| QR scanning (camera) | rxing, in alloy |

`fs`, `http`, `path`, `process`, `isolate`, `image`, `svg` and `tty` stay
unconditional: they are the foundation an app and the runtime itself stand
on, not optional capabilities. Video decoding and ktx2 already are features
at every layer; they only move from knobs to `default`.

## Stage 1: wasm, and the mechanism

- **forge**: `wasm = ["dep:wasmi"]`, `wasm-native = ["wasm",
  "dep:wasmtime"]`. wasmtime becomes a target-specific optional dependency,
  declared only where Cranelift has a backend and runtime codegen is
  allowed (64-bit x86 and arm, not iOS), so on armeabi-v7a and x86 Android
  `wasm-native` resolves to wasmi. `forge::wasm` exists only under `wasm`;
  the facade picks the native backend by the feature plus the same target
  condition, written once in code (a cfg alias from forge's build.rs) beside
  the one in the manifest.
- **flux**: `wasm = ["forge/wasm"]`; the `flux:wasm` registration and the
  `"wasm"` capability under it.
- **lattice**: `wasm` and `wasm-native` passthroughs, in `default` and in
  `go`. The process `WasmEngine` (`lattice/src/lib.rs`, handed to the
  engine builder and the store's install pre-warm in `lattice/src/go/`) is
  absent without the feature; the plumbing already carries it as an
  `Option`.
- **Mechanism**: lattice gains `default`, and `video` and `ktx2` move into
  it in the same stage, so runtime.mk has one mechanism rather than knobs
  for two features and the manifest for a third. `VIDEO`, `KTX2`,
  `WASM_NATIVE` and the per-ABI block leave runtime.mk, `lattice/Makefile`
  and `.github/workflows/ci.yml` (`flux/Makefile` keeps its modules on by
  its own features); docs/runtime/index.md swaps the knob sentence for the
  Cargo.toml shape.
- **Check**: a runtime with `default-features = false` links neither
  wasmi nor wasmtime (`cargo tree`), has no `"wasm"` capability, and an
  import of `flux:wasm` fails with a clear error; the stock client and
  runtime are unchanged; one size row for wasm entirely off.

## The build-time check

With the player carrying everything, a trimmed runtime's missing module
otherwise surfaces as a failed import on the device. `sol pack` for an app
on a custom runtime checks the app's `flux:*` imports (the bundler already
sees them) against the runtime's capability list, asked of the built
binary (`flux::capabilities()` behind a flag), and fails naming the module
and the lattice feature to add. Not needed for stage 1 to land; wanted
before a trimmed runtime is used in earnest.

## Done looks like

- Each module in the table is a lattice feature, in `default` and in `go`;
  a runtime with `default-features = false` links none of their
  dependencies.
- The stock `make client` and `make runtime` output is unchanged.
- runtime.mk has no capability knobs; docs/runtime shows the Cargo.toml
  shape.
- `sol pack` refuses an app importing a module its runtime lacks.
- CI compiles one build with the features off, so a cfg break reaches no
  release (the `ktx2` line in tiny.md is the same gap).
- The size table in runtime-optimization gets one row per module from the
  with-and-without builds.

## Not in scope

- Changing what the stock binaries ship. Which modules a default runtime
  carries is a product decision the size numbers inform; this item only
  makes the choice possible.
- Feature-gating the gui plugins (`alloy_plugins/`) beyond QR scanning:
  lattice is the renderer, and a headless flux without `gui` already
  exists. The spatial core in particular stays: both `@solidrt/2d` and
  `@solidrt/3d` stand on it.
- Speech: it left the tree instead of becoming a feature
  ([speech-out-of-tree](../done/speech-out-of-tree.md)).
