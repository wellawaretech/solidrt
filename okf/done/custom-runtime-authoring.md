---
title: Make a custom runtime cheap to set up and ship
description: Every custom runtime repeated the same boilerplate (manifest, entries, toolchain, page flags, Android environment, staging) that sol's own hint assumed but nothing wrote, the stock program's four build goals lived in lattice's Makefiles where a custom runtime could not share them, and the Native code docs left out the staging folder names, C++ on Android, build cost and stripping; now one build base in the CLI package that every runtime's one-line Makefile includes, sol init --runtime, and the docs.
created: 2026-10-11
---

# Make a custom runtime cheap to set up and ship

Done 2026-10-11. [runtime-extension-modules](runtime-extension-modules.md)
made a custom runtime possible and deferred its scaffolding "until a second
project wants it"; the vision demo was that project, and what setting one
up and shipping it ran into is below, with what closed it. The tag
dependency itself is
[lattice-tag-dependency-bundles](../plans/lattice-tag-dependency-bundles.md),
the module names [runtime-module-namespace](../backlog/runtime-module-namespace.md).
The module author's part (the toolkit through `lattice::flux`, the missing
binary as an error, discovery stopping at another project, the rquickjs
alias) was done the same day, before this.

## What it ran into

- **Setup: no scaffold.** Every custom runtime wrote the same files: the
  manifest with its two bins and the `go` feature, `lib.rs` with
  `android_entry!`, the two `main`s, the toolchain pin with the Android
  targets, the 16 KB page link flags, the profile overrides, the Android
  environment `cargo ndk` needs (the NDK variables, a copy of the ABI
  toolchain wrapper per target, the ABI), and the staging layout. sol
  assumed a convention, "build it with `make -C runtime`", that nothing
  wrote and the docs did not describe.
- **Building: the four goals lived in lattice's Makefiles**, mixed with the
  checkout's release machinery, so a custom runtime rewrote them (the
  vision demo's build.sh, the tower demo's Makefile), each with its own
  drift.
- **Shipping: the docs left out** the staging folder names (`<triple>`
  read as a Rust triple; sol looks for the npm platform name), the C++
  runtime on Android, the build cost, and stripping (a derived APK took an
  81 MB `libmain.so` as given).

## Decision

The constraint that shaped it: a scaffolded file never updates. Every
line a scaffold writes is frozen at that release, so the scaffold writes
only what is the project's own, and everything else is included from a
base that moves with the app.

- **The base is the CLI package**, `@solidrt/cli/runtime/`, versioned
  with the lattice tag: `runtime.mk` (the four goals, the feature
  plumbing, the host blocks, the Android environment and staging, help),
  `platform.mk` (the host and ABI maps, the rename-aside staging; the
  flux, lattice and website Makefiles include it from there too),
  `config.toml` (cargo's config: the page-size link flag, the rpaths, the
  profiles, moved out of the root Cargo.toml so one file serves both),
  `rust-toolchain.toml`, and the two cmake toolchains. A runtime's
  Makefile is one line, `include ../node_modules/@solidrt/cli/runtime/runtime.mk`,
  in the checkout's `runtime/` and in an app alike (the checkout's
  node_modules links `packages/cli`).
- **The base carries the workspace settings onto the build itself**:
  `RUSTUP_TOOLCHAIN` from the pin (rustup auto-installs it; the Android
  goals `rustup target add`, 0.4 s when present), `--config` with the
  package's config.toml, the cmake files named to `cargo ndk`, and
  lattice's features as `lattice/<feature>` on the command line (a bin
  may `required-features = ["dep/feature"]`, verified; the crate keeps
  only its own `go`, because `#[cfg(feature = "go")]` in `program()`
  cannot see a dependency's feature). The manifest declares no
  profiles, no toolchain, no flags, and a feature lattice adds later
  needs no edit.
- **The version is checked, not plumbed.** `SOLIDRT_VERSION` is the
  lattice tag read from Cargo.toml; the build fails when the tag and the
  installed `@solidrt/cli` differ, naming both. `CARGO_PKG_VERSION`
  cannot serve: the release workflow bumps the crate versions in the
  publish job, not in the tag, so the tag's manifests say 0.0.0.
- **The stock program embeds the screens as a custom one does**: `make
  player-bundle` writes into `packages/cli/player/`, `runtime/resources/`
  is gone, release.yml only bundles, and `lib.rs` has the same
  `include_str!` lines in both.
- **`libc++_shared.so` is staged by need**: `llvm-readelf -d` over the
  staged libs, copied when one lists it as NEEDED (the stock build with
  speech, any C++ module), removed otherwise. One rule in the docs
  instead of two cases.
- **`sol init --runtime`** writes the runtime (Makefile, Cargo.toml,
  lib.rs, a starter `runtime:hello`, the two bins, `types/hello.d.ts`, a
  test in `tests/`) into a new app or the one at the cwd, and the three
  edits (package.json's runtime key, tsconfig's include, `.gitignore`).
  The writer is pure (`init/runtime.ts`) and tested without a file
  system; the two base files that must also exist at the checkout root
  (the pin, the config) are checked against their originals by
  `scripts/check-runtime-base.ts` in CI (a flux test cannot: the test
  sandbox is its working directory).
- **Stripping**: `PROFILE` keeps its meaning for the desktop binaries
  (`release` keeps symbols for the dev loop, `release-opt` strips). The
  Android libs are staged stripped at every profile: the folder is what
  the APK carries, sol swaps the libs in as they are, and the checkout's
  Gradle strip never runs on a derived APK; the prebuilt impeller alone
  carries 220 MB of symbols. No strip in sol.
- **Rejected**: copying the Makefile and the settings into the scaffold
  (frozen copies; the drift this item is about), symlinks from the
  package to the checkout's files (npm pack drops them, Windows checkouts
  turn them into text), baking the version into the Makefile at init (two
  places to bump, drift between them), `CARGO_TARGET_*_RUSTFLAGS` in the
  environment instead of `--config` (works, but the profiles would still
  need the manifest).

## Measured

On this laptop, release profile, eight cargo jobs, 2026-10-11:

- Full builds through the new chain, zero warnings: the player 5m02s, the
  runtime 9m12s (a target dir of its own), the Android player 5m25s
  (sharing the desktop player's target dir). The move of the cache from
  `lattice/target` to `runtime/target` cost one full rebuild: cargo's
  fingerprints did not survive it.
- Android staging, stripped vs as built: libmain.so 70.5 MB to 52.4 MB,
  libimpeller.so 228 MB to 7.0 MB, libSDL3.so 12.3 MB to 2.4 MB,
  libc++_shared.so 9.3 MB to 1.4 MB. The stock libmain.so links
  libc++_shared.so without speech (through impeller), so the by-need rule
  stages it for the stock build too.
- A scaffolded app (`sol init tmp/scaffold-check --runtime`, lattice
  switched to a path dependency, node_modules linked to the checkout's):
  its player built in 7m28s pointed at the checkout's target dir (its own
  lockfile resolves differently, so most crates rebuilt; sharing a cache
  across workspaces needs a copied lockfile too), a module-only change
  rebuilt in 15 s, and `sol test` ran both its tests on that player,
  `runtime:hello` imported and answering. Deleted afterwards.
- `rustup target add` for a target already present: 0.4 s, offline.

## Follow-up

A CI job that scaffolds, swaps in a path dependency and builds, on the
native test matrix: the one proof that the scaffold builds as shipped
(okf/tiny.md, "Scaffolded runtime in CI").
