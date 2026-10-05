---
title: Register app-specific native modules without forking the runtime
description: The route for native code is a custom runtime build, but lattice builds its FluxEngine inside start_with, the stock main is not reusable and sol only knows the stock binaries, so a developer who needs one more module has to fork; take extra flux plugins and per-frame hooks on the entry points, expose the stock main as a function, and let sol pick up a project's own runtime binaries, so a custom player is a cargo project beside the app depending on a tagged lattice; speech recognition is the worked example.
created: 2026-10-03
---

# Register app-specific native modules without forking the runtime

## Symptom

With `flux:ffi` gone ([ffi-module-removal](../done/ffi-module-removal.md)),
the sanctioned route for a developer who needs a native library, a platform
SDK or engine state is a custom runtime build. The pieces for a build that
does not fork are nearly there: lattice is a library crate, the repo is
public so a cargo git dependency pinned to a tag reaches it without a
checkout the developer ever edits, the native pieces underneath (Impeller as
prebuilt libraries, SDL built from source by its crate) need no manual
setup, and `FluxEngine::builder()` already takes plugin closures
(`flux/examples/custom_plugin.rs` is the worked example). What is missing is
the last mile, in three places:

- `start`, `start_render` and `start_tests` build the engine inside
  `start_with` with no way to hand extra plugins in, and the frame tick in
  `runtime.rs` calls the speech tick by name behind its feature flag.
- The `solidrt` binary's `main.rs` is a few hundred lines of payload, link
  and flag handling that a custom runtime has to copy verbatim and keep in
  step with.
- `sol` resolves binaries from `SOLIDRT_HOME/dist/<triple>` and then the
  platform npm packages (`packages/cli/src/lib/artifacts.ts`,
  `packages/cli/src/server/binaries.ts`); a project's own runtime has no
  place in that order.

So "add one module" today means editing lattice, which is a fork, with every
rebase a fork implies.

Why this is the route and not a loadable extension: on Android the only
native code that may run is what arrived inside the APK, and iOS forbids
loading downloaded code outright. A native extension there is a custom build
by definition, so the mechanism to get right is the build, not a loader.

The same flow serves subtraction: a custom player that leaves modules out.
That works exactly for the modules that are a cargo feature all the way down
(forge dependency, flux registration, capability entry), which today is
`video`, `ktx2`, `wasm-native`, `speech` and `test` and nothing else;
[module-feature-flags](module-feature-flags.md) is the prerequisite.

## Shape

- **The hook.** The lattice entry points take an `Extensions` value: extra
  flux plugins, the same closure type the builder takes, forwarded beside
  the built-in ones, plus per-frame ticks. A module registers under its own
  `flux:*` name; `Flux.capabilities` lists it like any built-in, so an app
  checks for it by name and a stock runtime fails with a clear message
  instead of an import error. The plugin closure receives the alloy context
  the way the speech plugin does today, since a module that writes into the
  render tree or the spatial arena reaches alloy, not just flux.
- **The frame hook.** A module that reads a device or paces work by the
  frame needs a call per frame, where `runtime.rs` runs
  `flux::gui::frame::advance` and today the speech tick. The ticks in
  `Extensions` run at that spot with the JS context. Without it an extension
  falls back to a timer or a tokio task, off the frame cadence and with the
  JS-thread discipline left to the author.
- **The stock main as a function.** `lattice::run(extensions)` is today's
  `main.rs` moved into the library: payload and link handling, the flag
  parsing, the render and test runners. The stock `solidrt` and
  `solidrt-go` binaries call it with no extensions; a custom runtime's
  `main.rs` is one call with its module registered. One `main.rs` yields
  both binaries by feature, so the module is present under `sol run` (the
  player) and in `sol pack` output (the runtime) alike.
- **The project.** A cargo project beside the app with lattice as a git
  dependency pinned to a release tag, `default-features = false` and the
  feature list it wants, as the Makefile's cargo line spells today. The
  crate depends on the module's library (Rapier, a platform SDK binding), and
  builds the two binaries plus the cdylib Android loads.
- **Types.** The module's `.d.ts` lives in the project, in the same shape as
  `packages/flux-types/modules/*.d.ts`, included through `tsconfig.json` so
  the editor and `sol check` see it. When a module graduates into the repo,
  the file moves into `@solidrt/flux-types`.
- **How sol finds it.** A `solidrt.runtime` key in `package.json` names a
  directory laid out exactly like `SOLIDRT_HOME/dist`: `<triple>/solidrt`,
  `<triple>/solidrt-go` (with the GL libraries beside them on Windows and
  macOS), `android/<abi>/solidrt-go.apk`, `android-runtime/<abi>/solidrt.apk`.
  `resolveBinary`, `requireBinary`, `resolveApk` and `resolveRunnerApk`
  check it first; the platform packages stay the fallback for targets the
  project did not build. Same layout, so the resolver change is one more
  root, not a new scheme.
- **Android.** The shell is the Gradle project under `lattice/android/`,
  whose cmake step builds lattice as a cdylib. `sol pack` already patches
  APKs entry by entry with the 16 KB alignment Android 15 requires
  (`packages/cli/src/pack/android/zip.ts`), so the fit is for `sol android`
  and `sol pack --apk` to take the project's `.so` and swap it into the stock
  player and runner APKs. The project then never carries a copy of the
  shell. This is the piece most likely to need design: whether the shell's
  Java side has anything per-build in it decides if a swap is enough.
- **The build matrix.** The remote builders already produce the stock
  runtime for every target; the custom binary is the stock binary plus one
  crate, so the same flow applies, with the project's `dist/` as the
  destination.

## Project layout

Modeled on the tower-toppling demo, with its wasm crate replaced by the
runtime crate; the app itself does not change shape.

```
my-game/
  package.json              as today, plus "solidrt": { "runtime": "runtime" }
  tsconfig.json             includes types/
  .mcp.json
  AGENTS.md
  assets/
  src/
    index.tsx
    physics.ts              the JS boundary: import { World } from "flux:physics"
  types/
    physics.d.ts            declare module "flux:physics"
  runtime/
    Cargo.toml              lattice = { git, tag, default-features = false, features = [...] }
    Cargo.lock              committed: it is a binary
    rust-toolchain.toml     the pinned channel
    .cargo/config.toml      android targets, profile settings
    Makefile                desktop binaries and the android .so, staged into dist/
    src/
      main.rs               fn main() { lattice::run(Extensions::new().module(physics::install).tick(physics::tick)) }
      physics/
        mod.rs              the core: native Rust types only
        plugin.rs           the flux plugin: marshals JS <-> core, nothing else
    dist/                   gitignored; the SOLIDRT_HOME/dist layout
      linux-x64-gnu/        solidrt, solidrt-go
      win32-x64-msvc/       solidrt.exe, solidrt-go.exe, libEGL.dll, libGLESv2.dll
      android/arm64-v8a/    solidrt-go.apk
      android-runtime/arm64-v8a/  solidrt.apk
```

`runtime/src/physics/` keeps the split flux keeps between forge cores and
the plugin layers: domain logic in a module of native types, a thin plugin
over it. It is what lets the core be lifted into the repo later if the
module becomes a built-in.

## Traps

- Workspace-level settings do not travel with a git dependency. The exact
  version pins do, but the `[profile]` overrides (the optimized
  `rquickjs-sys` in dev, `release-opt`), the `[patch.crates-io]` entries and
  the `rust-toolchain.toml` pin do not. The project carries its own copies;
  the patch only matters under `speech`.
- Features unify across one cargo build: a custom player cannot have less
  than its own crates ask for. Lattice asks for nothing beyond
  `flux/compile` by default, so the stock set (`video`, `ktx2`,
  `wasm-native`) becomes the project's choice rather than a given.
- `lattice::VERSION` is read from `SOLIDRT_VERSION` at compile time with a
  fallback; a custom build should set it to the tag it depends on, or every
  log line and version check reports the fallback.
- A plugin closure that reaches alloy runs on the UI thread with the JS
  context; the speech plugin's session and tick shape is the model for
  anything that owns device state.

## Worked example: speech recognition

Speech is the proof the shape is right, because it is in the tree today as
the opt-in `speech` feature on lattice and splits along exactly these
lines:

- `lattice/src/speech.rs` (VAD, Whisper, wake word on a worker thread) uses
  nothing from lattice; it moves into the extension crate as is, and with
  it whisper.cpp's C++ build leaves the default runtime for good.
- `lattice/src/plugins/speech.rs` needs the JS context (the plugin
  closure), the microphone (`flux::gui::alloy_context` already hands any
  plugin the alloy context) and the per-frame tick (the frame hook above).
- Registration is the one `.module(...)` call on `Extensions`.

Done for the example means: the `speech` feature is gone from lattice, a
speech-enabled SolidRT is a cargo project of a manifest (lattice plus
whisper-rs and livekit-wakeword), the recognizer and a main that registers
the plugin and the tick, and the console app checks `Flux.capabilities` for
`speech` as it would for any module.

## Done looks like

- A cargo project outside the repo, depending on a tagged lattice, adds one
  `flux:*` module with a per-frame tick and runs an app that imports it, on
  desktop through `sol run` and `sol pack`, and in an APK. Speech is that
  module (above).
- `sol init` can scaffold the `runtime/` crate, or the docs show the
  manifest and main in full.
- The "Native code" section of `docs/runtime/index.md` says what to run.
- `flux/examples/custom_plugin.rs` stays the flux-level example; a lattice
  example beside it shows the same thing one layer up.

## Order

Desktop first: the hook, `lattice::run`, and the `solidrt.runtime` root in
the resolvers give a working custom player from a manifest and a short main.
Android stays a checkout build until the APK swap is designed.

## Not in scope

- A stable C ABI for the engine or a dlopen-based extension loader (see the
  symptom for why).
- Publishing the crates to crates.io: the git dependency reaches a public
  repo, and the path dependencies carry no `version` fields, which
  publishing would need. Revisit only if the git clone proves a problem.
- Anything on the `.solapp` side beyond the capability check: the payload
  stays the same bytes everywhere, and the runtime it needs is a deployment
  fact, not an app fact.
