---
title: Register app-specific native modules without forking the runtime
description: The route for native code is a custom runtime build, but lattice builds its FluxEngine inside start_with and registers the built-in modules in a fixed list, so a developer who needs one more module has to fork; take extra flux plugins and per-frame hooks on the entry points and document the cargo project that depends on lattice instead of cloning it; speech recognition is the worked example.
created: 2026-10-03
---

# Register app-specific native modules without forking the runtime

## Symptom

With `flux:ffi` gone ([ffi-module-removal](../done/ffi-module-removal.md)),
the sanctioned route for a developer who needs a native library, a platform
SDK or engine state is a custom runtime build. The pieces for a build that
does not fork are nearly there: lattice is a library crate and the `solidrt`
binary is a thin `main.rs` over `lattice::start`; `FluxEngine::builder()`
already takes plugin closures (`flux/examples/custom_plugin.rs` is the worked
example). What is missing is the last step: `start`, `start_render` and
`start_tests` build the engine inside `start_with` with no way to hand extra
plugins in, and the built-in modules are a fixed list in
`flux/src/plugins/mod.rs`. So "add one module" today means editing those
files, which is a fork, with every rebase a fork implies.

Why this is the route and not a loadable extension: on Android the only
native code that may run is what arrived inside the APK, and iOS forbids
loading downloaded code outright. A native extension there is a custom build
by definition, so the mechanism to get right is the build, not a loader.

## Shape

- **The hook.** The lattice entry points take a list of extra flux plugins,
  the same closure type the builder takes, forwarded beside the built-in
  ones. A module registers under its own `flux:*` name; `Flux.capabilities`
  lists it like any built-in, so an app can check for it by name and a stock
  runtime fails with a clear message instead of an import error.
- **The frame hook.** A module that reads a device or paces work by the
  frame needs a call per frame, where `runtime.rs` runs
  `flux::gui::frame::advance` and today the speech tick behind its feature
  flag. The entry points take a list of per-frame callbacks beside the
  plugins, run at that spot with the JS context. Without it an extension
  falls back to a timer or a tokio task, off the frame cadence and with the
  JS-thread discipline left to the author.
- **The project.** A cargo project with lattice as a git dependency pinned
  to a release tag, and a `main.rs` that is the stock runner plus the extra
  plugins. Whether the stock `main.rs` can be reused as a function rather
  than copied decides how thin that file is; today the payload and link
  handling live in the binary's own main.
- **Types.** The module's `.d.ts` beside the built-in ones, in the same
  shape as `packages/flux-types/modules/*.d.ts`, so the editor and `sol
  check` see it.
- **The build matrix.** The remote builders already produce the stock
  runtime for every target; the custom binary is the stock binary plus one
  crate, so the same flow applies. `sol pack` and `sol android` have to
  accept a runtime that is not the stock one, which is the piece most likely
  to need design.

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
- Registration is the one `builder.plugin(...)` call the plugin hook
  exposes.

Done for the example means: the `speech` feature is gone from lattice, a
speech-enabled SolidRT is a cargo project of a manifest (lattice plus
whisper-rs and livekit-wakeword), the recognizer and a main that registers
the plugin and the tick, and the console app checks `Flux.capabilities` for
`speech` as it would for any module.

## Done looks like

- A cargo project outside the repo, depending on a tagged lattice, adds one
  `flux:*` module with a per-frame tick and runs an app that imports it, on
  desktop and in an APK. Speech is that module (above).
- The "Native code" section of `docs/runtime/index.md` says what to run.
- `flux/examples/custom_plugin.rs` stays the flux-level example; a lattice
  example beside it shows the same thing one layer up.

## Not in scope

- A stable C ABI for the engine or a dlopen-based extension loader (see the
  symptom for why).
- Anything on the `.solapp` side beyond the capability check: the payload
  stays the same bytes everywhere, and the runtime it needs is a deployment
  fact, not an app fact.
