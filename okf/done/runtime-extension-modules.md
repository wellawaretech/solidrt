---
title: Register app-specific native modules without forking the runtime
description: The route for native code is a custom runtime build, but lattice built its engine with no way to hand modules in, the stock main was not reusable and sol only knew the stock binaries, so one more module meant a fork; lattice now takes Modules on its entry, the stock main is lattice::main, sol takes a project's own runtime from a package.json key and derives its Android APKs from the stock ones; the tower-toppling demo's Rapier physics (flux:physics3d and flux:physics2d, stepping in the frame tick) is the worked example.
created: 2026-10-03
completed: 2026-10-06
---

# Register app-specific native modules without forking the runtime

## Symptom

With `flux:ffi` gone ([ffi-module-removal](../done/ffi-module-removal.md)),
the sanctioned route for a developer who needs a native library, a platform
SDK or engine state is a custom runtime build. The pieces for a build that
does not fork were nearly there: lattice is a library crate, the repo is
public so a cargo git dependency pinned to a tag reaches it without a
checkout the developer ever edits, the native pieces underneath (Impeller as
prebuilt libraries, SDL built from source by its crate) need no manual
setup, and `FluxEngine::builder()` takes plugin closures
(`flux/examples/custom_plugin.rs`). What was missing was the last mile, in
three places:

- `start`, `start_render` and `start_tests` built the engine inside
  `start_with` with no way to hand extra modules in, and the frame tick in
  `runtime.rs` called the speech tick by name behind its feature flag.
- The `solidrt` binary's `main.rs` was a few hundred lines of payload, link
  and flag handling a custom runtime would have to copy and keep in step
  with; and the Android `SDL_main` lived inside the lattice library, which
  a dependent's cdylib exports as its own (a dependency's `#[no_mangle]`
  symbols are exported from a cdylib; the stock lattice cdylib exports
  blake3's), so a custom cdylib could neither replace it nor define one.
- `sol` resolved binaries from `SOLIDRT_HOME/dist/<triple>` and then the
  platform npm packages; a project's own runtime had no place in that order.

Why this is the route and not a loadable extension: on Android the only
native code that may run is what arrived inside the APK, and iOS forbids
loading downloaded code outright. A native extension there is a custom build
by definition, so the mechanism to get right is the build, not a loader.

The same flow serves subtraction: a custom runtime that leaves modules out.
That works exactly for the modules that are a cargo feature all the way down
(forge dependency, flux registration, capability entry), which today is
`video`, `ktx2`, `wasm-native`, `speech` and `test` and nothing else;
[module-feature-flags](../backlog/module-feature-flags.md) is the
prerequisite.

## Shape, as built

- **flux owns the per-frame tick.** `flux::gui::frame::on_advance(ctx,
  reason, tick)` registers a hook in the gui state, per engine, run at the
  end of `advance` after the devices with the frame's app time; returning
  true demands the next frame under `reason`, the way a running player does,
  so the loop ticks while a module has motion and a settle waits for it.
  Lattice's runtime no longer knows about any tick: the speech plugin
  registers its own. `flux::add_capability(ctx, name)` appends to
  `Flux.capabilities` (the gui's own names go through it too), and the
  builder's `module_override` is `module`: nothing was ever overridden.
- **lattice: `Modules`, `main`, `android_main`.** `Modules::new().add(name,
  def)` registers an rquickjs `ModuleDef` under a `flux:*` name on every
  engine the runtime builds and lists its short name in `Flux.capabilities`
  and in the client's report to the dev server; `.engine(f)` is the
  primitive under it, anything applied to every engine's builder. The stock
  `main.rs` moved into the library as `lattice::main(modules)`; the two
  `SDL_main` bodies became `android_main(argc, argv, modules)`, and the
  `android_entry!` macro expands to the `SDL_main` over it, nothing on
  other targets. The stock cdylib's entry is that macro behind the
  `android-entry` feature, which the Makefile turns on and a custom runtime
  leaves off. `compile = ["flux/compile"]` is a named feature, since a
  dependent cannot spell `flux/compile`. Lattice re-exports `flux`, and
  flux exports `OptArg`, so a module is written against one dependency with
  the project's own optional-argument type. The crate's public surface is
  now the entry (`main`, `android_main`, `android_entry!`, `Modules`,
  `VERSION`, `flux`) and `speech` under its feature; everything the moved
  main used went `pub(crate)`. `lattice/examples/custom_runtime.rs` is the
  shape at its smallest, a `flux:whoami` module with a tick.
- **sol: a project runtime root.** `"solidrt": { "runtime": "runtime/dist"
  }` names a directory in the `SOLIDRT_HOME/dist` layout. `lib/runtime.ts`
  resolves it once per process: `solidrt`, `solidrt-go` and the Android
  APKs come from it first (the tooling binaries `flux`, `fluxc`, `fluxrt`
  stay sol's own), a target the project did not build falls back to the
  stock binary with a notice. The server side reads the same key in
  `server/mode.ts` and spawns the client from it. The APKs are derived:
  `android/<abi>/*.so` swapped into the stock Player APK (`swapLibs`,
  each entry keeping the base's packing, versionName suffixed with the
  libs' digest, re-signed), written beside the libs and rebuilt when a lib
  or the base is newer; `android-runtime/<abi>/` the same over the runner.
  A 64-bit lib under 16 KB LOAD alignment is refused before it goes in
  (`pack/android/elf.ts`, tested). `sol android` reads the version it
  expects off the APK it would install, so a rebuilt custom Player
  installs; `androidPackageVersion` is gone. `fail` moved to `lib/fail.ts`
  to break the import cycle the project-aware resolver would have made.
- **Docs.** The "Native code" section of `docs/runtime/index.md` shows the
  manifest, the entry files, the staging layout, the key and the traps.

## The worked example: the tower-toppling physics

`~/solidrt/demoes/tower-toppling/runtime/` (outside the repo): a crate over
lattice with `flux:physics3d` and `flux:physics2d`, Rapier 0.36 in both
dimensions, replacing the demo's wasm crate and its isolate worker. Each
module is a core of native types (`physics3d/mod.rs`: a `PhysicsWorld`,
dense groups, the fixed-step clock) and a plugin of three rquickjs classes
over it (`World`, `Group`, `Body`); the two share `physics/` (the stepper,
the groups, the record layout, the option marshalling). The world ticks in
the frame protocol: the app time's delta accumulated in whole steps, at
most `maxSteps` per frame, the rest dropped; awake bodies demand the next
frame. The one per-frame call is `group.poses(target, { stride, position,
rotation })`, every body's pose written into the app's own record layout:
the 3d demo's 44-byte instanced records (one call and one `setRecords`
per group), the 2d screen's record-layer sprites (`records(layer)`,
position at 0, rotation at 8). Bodies have a dense index per group; a
destroy swap-removes and returns the index freed, so the app copies the
last record over it. The types live in `types/physics3d.d.ts` and
`physics2d.d.ts`. The 2d screen is `src/tower2d.tsx`, the same tower on a
record layer through the raw view API (`<SpriteLayer>` wraps node layers
only).

## Traps

- Workspace-level settings do not travel with a git or path dependency.
  The project repeats the `[profile]` overrides (the optimized
  `rquickjs-sys` in dev, build scripts unstripped), the toolchain pin with
  the Android targets, and the whole `.cargo/config.toml`: the rpath the
  binaries find the GL libraries beside them by (`@executable_path` on
  macOS, `$ORIGIN` on Linux), the macOS deployment floor, the 16 KB
  page-size link flag per Android target, and git over the system git.
- Features unify across one cargo build: a custom runtime cannot have less
  than its own crates ask for; the stock set (`video`, `ktx2`,
  `wasm-native`) is the project's choice. The two binaries are two cargo
  runs (`--features go` for the player).
- The rquickjs class and methods macros resolve their crate by name, so
  the project names `rquickjs` (the same pin, `features = ["macro"]`)
  beside lattice.
- `Cargo.lock` is the project's own. Started fresh it resolves newer
  transitive versions than the checkout's and the project builds
  everything from scratch; seeded from the checkout's lockfile it has the
  stock runtime's exact transitive versions. Even then a project that adds
  crates (rapier turns on features of bottom crates like num-traits) gets
  a full first build: rustflags, features and dependencies are all part of
  a crate's fingerprint, so the checkout's cache is not shared in general.
- `lattice::VERSION` is read from `SOLIDRT_VERSION` at compile time; the
  project's Makefile sets it, or every log line reports the fallback.
- An Android build needs what `lattice/Makefile.android` hands cmake:
  `ANDROID_NDK_HOME` and `ANDROID_NDK_ROOT`, a copy of the checkout's
  `android-abi.toolchain.cmake` wrapper as `CMAKE_TOOLCHAIN_FILE_<rust
  target>`, and `CMAKE_ANDROID_ARCH_ABI`. With only the NDK home set,
  sdl3-sys fails in `Android-Determine.cmake`; with the NDK's own toolchain
  file, forge's video codecs (a cmake build that passes no ABI) fail their
  compiler test, configured for armeabi-v7a against aarch64 flags.
  And a cmake build directory keeps its first configuration: after a
  toolchain change, the `build/<crate>-*` directories under the target
  must go, or the compiler test keeps failing the old way.
- Never `pkill -f` a build from a script whose own command line contains
  the pattern: it kills the shell (exit 144), as it did once in this work.

## Done looks like

- A cargo project outside the repo, depending on lattice, adds `flux:*`
  modules with a per-frame tick and runs an app that imports them, on
  desktop through `sol run`, `sol test` and `sol pack`, and in an APK.
- The docs say what to run; `sol init` scaffolding of `runtime/` waits for
  a second project that wants it.

## Order and status

1. flux and lattice: built and verified. `lattice/examples/custom_runtime.rs`
   staged as a scratch project's runtime ran its app test under `sol
   test` (module import, capability, three ticks for three stepped frames)
   and its app under `sol run` with the tick counting and the client
   reporting `whoami` in its capabilities.
2. sol: built; the whoami check above ran through the new resolvers on both
   the bun and the server side; the ELF check has tests.
3. The physics runtime: built and verified on desktop. The demo runs on its
   player under `sol run` (both modules in `Flux.capabilities`, the tower
   settling to zero frames, the world the standing demand while bodies
   move, a frozen clock stepping a shot one frame per `step` snapshot and
   holding it between), the bench numbers are in Findings, the 2d screen
   runs the same way, and `sol pack` wrote a single-file app over the
   production runtime that starts and runs. The runtime crate is at
   `~/solidrt/demoes/tower-toppling/runtime/`, outside the repo.
4. Android: `make -C runtime android` cross-builds both libraries (after the
   two cmake traps in Traps), and sol derives the Player and runner APKs
   from the stock ones with them, re-versioned (`1.0+r<hash>` over the
   checkout's Player) and re-signed with sol's development key. Installed
   on the SM-T500 tablet (arm64-v8a) with `sol android --install`: the
   first install over a gradle-built Player.dev needs an uninstall once,
   since the two are signed with different keys (sol says so and stops in
   a non-TTY). The demo ran on the tablet: both modules in the client's
   capabilities, the world the standing demand while bodies moved, rest
   after the rain, the bench numbers in the findings. The runner leg
   (`sol pack --apk` on the device) was not run: the derived runner's base
   is the published 0.0.67 package, the checkout has no staged runner.
   Open, filed as its own item: on a cold start of the Player the demo
   casts no shadows on the tablet
   ([android-cold-start-shadow-loss](../backlog/android-cold-start-shadow-loss.md)).

## Not in scope

- A stable C ABI for the engine or a dlopen-based extension loader (see the
  symptom for why).
- Publishing the crates to crates.io: the git dependency reaches a public
  repo, and the path dependencies carry no `version` fields, which
  publishing would need. Revisit only if the git clone proves a problem.
- Anything on the `.solapp` side beyond the capability check: the payload
  stays the same bytes everywhere, and the runtime it needs is a deployment
  fact, not an app fact.
- Speech: untouched beyond its tick registering through `on_advance`.
  Taking it out of the tree is its own item.

## Findings

Moved to [custom-runtime-findings](../notes/custom-runtime-findings.md).
