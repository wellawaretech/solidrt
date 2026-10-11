---
title: Make a custom runtime cheap to set up, write and ship
description: Every custom runtime repeats the same boilerplate (manifest, entries, toolchain, page flags, Android environment, staging) that sol's own hint assumes but nothing writes, every native module rebuilds flux's unexported promise-with-hold helpers, a missing binary falls back to the stock player with one quiet line, sol test sweeps up a vendored checkout's suites, and the Native code docs leave out the staging folder names, C++ and OpenMP on Android, build cost and stripping; a runtime scaffold, an exported module toolkit and the missing docs.
created: 2026-10-11
---

# Make a custom runtime cheap to set up, write and ship

[runtime-extension-modules](../done/runtime-extension-modules.md) made a
custom runtime possible, and deferred `sol init` scaffolding of `runtime/`
"until a second project wants it". The gaps below are what setting one up,
writing a module and shipping it still run into. The tag dependency itself is
[lattice-tag-dependency-bundles](../plans/lattice-tag-dependency-bundles.md), the
module names are [custom-module-namespace](custom-module-namespace.md).

## Setup: no scaffold

Every custom runtime writes the same files, and they take most of the setup
time: the manifest with its two bins and the `go` feature, `lib.rs` with
`android_entry!`, the two `main`s, the toolchain pin with the Android
targets, the 16 KB page link flags, the profile overrides, the Android
environment `cargo ndk` needs (the NDK variables, a copy of the
`android-abi.toolchain.cmake` wrapper per target, the ABI), and the staging
layout. sol already assumes a convention: with the runtime folder missing it
says "build it with `make -C runtime`" (`packages/cli/src/lib/runtime.ts`),
but the docs describe no Makefile and nothing writes one.

Shape: `sol init --runtime` writes `runtime/` with those files and a
Makefile whose goals build and stage each target (desktop player and
runtime, Android player and runtime), the one sol's hint names. The
manifest's lattice dependency is written at the CLI's own release, so it
starts in step with the app.

## Writing a module: the toolkit is private

- `with_in_flight`, `with_standing` and the `JsBytes` helpers (`bytes_of`
  and friends) live in `flux::plugins::marshal` and are not exported, so a
  module rebuilds the promise-with-hold shape every async binding needs
  from `flux::hold_engine`, `rquickjs::promise::Promised` and
  `flux::JsResult`. Export what a module needs through `lattice::flux`.
- The manifest names `rquickjs` at lattice's exact pin because its macros
  resolve their crate by name. Every macro takes a `crate = "..."` option
  (an identifier, not a path), so `use lattice::flux::rquickjs;` with
  `#[rquickjs::class(crate = "rquickjs")]` may make the dependency and its
  pin unnecessary; check the class, methods and trace macros that way and
  document whichever works.

## Building it: one Makefile for the stock program and a custom one

The stock program is `runtime/` in the checkout, the crate a custom runtime
is a copy of, but its four build-and-stage goals (desktop player and
runtime, Android player and runtime: one cargo build or cargo ndk run, a
strip and a copy into the dist layout sol reads) still live in
`lattice/Makefile` and its platform files, mixed with the checkout's release
machinery around them (the Gradle APK assembly, staging into the platform
packages, the flux binaries, notices, the font download, the player
bundling). The feature-flag plumbing (PROFILE, SPEECH, VIDEO, KTX2,
WASM_NATIVE, TEST, DIST) is what keeps them there.

Shape: the four goals move into `runtime/Makefile` with that plumbing, and
lattice's Makefiles call it for the cargo and staging steps. The scaffold
then writes the same Makefile with the modules slot empty, so the stock
program and a custom one build the same way, and the one sol's hint names
("build it with `make -C runtime`") exists. Pick this up right after
[lattice-tag-dependency-bundles](../plans/lattice-tag-dependency-bundles.md).

## Running it: the fallback is quiet

When a project declares a runtime and the binary for a target is missing,
`sol run` and `sol test` print one line ("No solidrt-go for linux-x64-gnu
... using the stock one") and go on, and the app then fails at its first
import of a custom module, which reads as a module bug. When the bundle
imports a `custom:*` module, a missing project binary is an error naming
the path sol looked for; without one, the fallback line stays a notice.

## Testing it: sol test sweeps up a vendored checkout

`sol test` walks every `tests/` folder under the cwd, skipping only
`node_modules`, `dist`, `target` and dot folders (`SKIPPED_DIRS`,
`packages/cli/src/test/main.ts`). A project that keeps SolidRT's sources
beside it (a vendored checkout for its runtime) gets SolidRT's own suites
in its run. Skipping every folder with a package.json of its own is too
much: the checkout's own packages each have one, and fixture projects under
`tests/` are walked on purpose. The rule is: outside a `tests/` folder, a
folder with its own package.json that is not one of the root's workspaces is
another project, and the walk does not enter it.

## Shipping it: what the docs leave out

docs/runtime "Native code":

- **The staging folder names.** `<triple>/solidrt-go` reads as a Rust
  target triple (`x86_64-unknown-linux-gnu`); sol looks for the npm
  platform name (`linux-x64-gnu`, `TRIPLE_MAP` in
  `packages/cli/src/lib/artifacts.ts`). Name the folders outright.
- **rquickjs closures.** A closure cannot take both `Ctx<'_>` and a value
  parameter (the lifetimes do not unify; flux works around it in
  `flux/src/alloy_plugins/camera.rs`), so a method with several arguments
  is a named generic function or takes one options object. One line.
- **The C++ runtime on Android.** A C++ module links `libc++_shared.so`.
  The stock APK carries it only because whisper-rs needs it
  (`lattice/Makefile.android`, `ndk-stage`); a module can ship its own in
  `android/<abi>/`, which sol swaps in with the rest. State both.
- **Static OpenMP on Android.** Libraries built with `-fopenmp
  -static-openmp` (most prebuilt NDK inference libraries) carry a static
  libomp that shuts down when the last thread that ran OpenMP exits, and
  aborts when a new thread starts it again
  (`__kmp_create_affinity_none_places`). A module that gives each job its
  own thread crashes the second time one is replaced; run such a library
  on one thread that lives as long as the process.
- **Build cost.** Four builds per release (desktop player and runtime,
  Android player and runtime), each compiling all of lattice: 8 to 14
  minutes each on a laptop, about 4 GB per linking job. Once lattice is
  built, a change to the module alone relinks in about a minute.
- **Stripping.** A release `libmain.so` is 81 MB unstripped and about 60 MB
  after `llvm-strip`, against the stock 45 MB, and the derived APK takes
  the `.so` as given. The documented profile overrides carry `strip =
  true`, or sol strips what it swaps into a derived APK; the scaffold's
  profile settles it.

## Done looks like

- `sol init --runtime` in an app writes a runtime that builds, stages and
  runs under `sol run`, `sol test` and `sol android` with no file edited
  but the module, desktop and Android.
- An async binding in a module uses the exported toolkit, and the scaffold
  needs no `rquickjs` line if the macros work through lattice.
- A missing project binary is an error when the app imports a custom
  module; `sol test` in a project with a vendored checkout runs the
  project's tests only.
- docs/runtime "Native code" covers the points above.
