---
title: Make a custom runtime cheap to set up and ship
description: Every custom runtime repeats the same boilerplate (manifest, entries, toolchain, page flags, Android environment, staging) that sol's own hint assumes but nothing writes, the stock program's four build goals live in lattice's Makefiles where a custom runtime cannot share them, and the Native code docs leave out the staging folder names, C++ on Android, build cost and stripping; a runtime scaffold, one Makefile for the stock program and a custom one, and the missing docs.
created: 2026-10-11
---

# Make a custom runtime cheap to set up and ship

[runtime-extension-modules](../done/runtime-extension-modules.md) made a
custom runtime possible, and deferred `sol init` scaffolding of `runtime/`
"until a second project wants it". The gaps below are what setting one up
and shipping it still run into. The tag dependency itself is
[lattice-tag-dependency-bundles](../plans/lattice-tag-dependency-bundles.md), the
module names are [runtime-module-namespace](runtime-module-namespace.md).
What writing a module and running or testing it ran into (the private
toolkit, the quiet fallback to the stock binary, `sol test` sweeping up a
vendored checkout, the rquickjs dependency and the closure and OpenMP traps)
is done: the toolkit is exported through `lattice::flux`, a missing project
binary is an error when the app imports a `runtime:*` module, discovery
stops at another project, and docs/runtime "Native code" covers the rest.

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
starts in step with the app, and it carries no `rquickjs` line: the macros
work through `lattice::flux::rquickjs` with `crate = "rquickjs"` (see
docs/runtime "Native code"). The Makefile sets `SOLIDRT_VERSION` in the
build environment: a runtime built without it reports `0.0.0-dev` in the
client info and every log line, which the docs mention and nothing
enforces.

Until the Makefile exists, sol's hint for a missing or unbuilt runtime
("build the runtime with `make -C runtime`", the notice and the error in
`packages/cli/src/lib/runtime.ts` and `server/binaries.ts`) names a file the
docs do not describe; this item makes the hint true.

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

## Shipping it: what the docs leave out

docs/runtime "Native code":

- **The staging folder names.** `<triple>/solidrt-go` reads as a Rust
  target triple (`x86_64-unknown-linux-gnu`); sol looks for the npm
  platform name (`linux-x64-gnu`, `TRIPLE_MAP` in
  `packages/cli/src/lib/artifacts.ts`). Name the folders outright.
- **The C++ runtime on Android.** A C++ module links `libc++_shared.so`.
  The stock APK carries it only because whisper-rs needs it
  (`lattice/Makefile.android`, `ndk-stage`); a module can ship its own in
  `android/<abi>/`, which sol swaps in with the rest. State both.
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
  but the module, desktop and Android, and no `rquickjs` line.
- `make -C runtime` builds and stages the stock program and a scaffolded
  one the same way.
- docs/runtime "Native code" covers the points above.
