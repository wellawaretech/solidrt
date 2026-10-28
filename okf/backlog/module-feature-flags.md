---
title: Put every optional flux module behind a cargo feature
description: A custom runtime can only leave out what is a cargo feature all the way down, and today sqlite, subprocess, mdns and QR scanning are unconditional (wasm, p2p and http are done); make each optional module a lattice feature, chosen in the runtime crate's Cargo.toml with the stock set as default and the player carrying all of them, starting with wasm.
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
- **Video is two features** (2026-10-15): `video`, the decoder, and
  `video-encode`, the encoder that is coming, which implies `video` (the
  same libvpx plus a WebM writer); the decoder stands alone. Both in
  `default`. `video-encode` is declared when the encoder lands, in the same
  forge/flux/lattice pattern, not before.

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

Landed 2026-10-15 as written below, with one refinement: the lane cfg is
`wasm_native` (from forge's build.rs), not the feature, since the feature
is on everywhere and resolves to wasmi where wasmtime is absent; flux
reports the `wasm-native` capability from `forge::wasm::LANE` for the same
reason. The flux binary names `ktx2,wasm-native,test` in its Makefile and
has no knobs either. Open from "Done looks like": the build-time check in
`sol pack`, and the size row.

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

## Stage 2: p2p

Landed 2026-10-28 in the shape above, as two independent features: an app
that serves the relay for others need not be a peer, and iroh-relay does
not depend on iroh. forge owns `p2p = ["dep:iroh", "dep:url"]` (the
endpoint, `forge/src/p2p/endpoint.rs`, and `http::accept_loop_p2p`) and
`p2p-relay` (iroh-relay, plus iroh-base for the token's key bytes);
`forge::p2p` exists under either. flux mirrors the split
(`forge_plugins/p2p/{endpoint,relay}.rs`): `flux:p2p` is registered under
either, `Endpoint` under `p2p`, `serveRelay` under `p2p-relay`, and the
`p2p` option of `serve` throws in a build without `p2p` rather than
serving TCP alone. lattice lists both in `default` and in `go` (the dev
tunnel needs the endpoint); the flux binary names both. Checked: no iroh
in `cargo tree` for lattice with `default-features = false` nor for a
relay-only forge; a flux binary without `p2p` lists no `"p2p"` capability
and fails to resolve `flux:p2p`. The size row is still open.

## Stage 3: http

Landed 2026-10-28, one feature with TLS inside: an app that declares
`internet: false` had the whole HTTP stack linked all the same. forge's
`http` owns reqwest (on rustls, ring and the webpki roots), hyper,
hyper-util, http-body-util and fastwebsockets, plus the small crates only
fetch, the cache and the server use (fastrand, blake3, serde, serde_json,
percent-encoding); it gates `fetch`, `cache`, `http` and `websocket`, and
the http(s) half of `forge::source`, whose `Source::Http` then fails to
open with a clear error. flux gates the `fetch` and `WebSocket` globals,
`flux:http` and the `"http"` capability; `Request`, `Response` and
`Headers` stay as data types, their server-only fields behind the feature.
core's image loader says a URL needs the feature rather than throwing a
bare ReferenceError. lattice lists `http` in `default` and in `go` (the dev
proxy and the version store fetch). The `flux` binary names it, and
`fluxrt`, the packed flux program's runner, now names `http,p2p`: its
default features had silently dropped both since p2p became a feature.

TLS still comes with `p2p` and `p2p-relay` (iroh's QUIC, the relay's
server), so a runtime free of TLS leaves out all three. Tying `internet:
false` to the feature belongs to the build-time check below. The size row
is still open.

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
