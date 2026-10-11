---
title: Make a lattice git dependency build at a release tag
description: A custom runtime depends on lattice by git tag, but the library embedded two bundles the JavaScript toolchain generates (untracked since v0.0.71), so lattice/build.rs panicked in cargo's checkout and the documented dependency could not build; the embed moves to a program crate (runtime/, the stock program and the shape of every custom one), the CLI package ships the bundles, and a custom runtime embeds them from its node_modules.
created: 2026-10-11
---

# Make a lattice git dependency build at a release tag

## Symptom

docs/runtime ("A custom runtime") has a custom runtime depend on lattice by
git tag. lattice embedded two generated bundles, the player and the BSOD
screen, made from `apps/player` by `make -C lattice player-bundle`. They
were untracked on 2026-10-09 (0284fadf) because every core, components or
player change carried a 500 KB generated diff, and `lattice/build.rs`
panicked when they were missing: right for a checkout, fatal for a git
dependency, whose checkout has no bun and no node_modules. v0.0.71 and
v0.0.72 are affected; up to v0.0.70 the tags still carried the files.

The way around it cost a custom runtime a vendoring script and two traps
(the vendored tree joining the project's workspace, and a copied lockfile
to keep the stock crate versions).

## Decision

The root cause is a Rust library whose build needs an output of the
JavaScript toolchain. Every fix that routes around that was rejected:

- **A second tag per release carrying the bundles.** Two tags per release,
  the docs naming a tag that is not the release tag, a tagged commit on no
  branch, and write access for the workflow. A permanent workaround.
- **Tracking the bundles again.** Bundling is deterministic (two runs are
  byte-identical), so it is possible, but the cost that got them untracked
  stays: a generated diff on most commits and a merge-conflict magnet.
- **An environment variable into build.rs.** A knob on every custom
  runtime build, and the lattice tag and the CLI version can disagree
  silently.
- **Loading the player from a file at runtime.** Removes the build coupling
  but gives up what embedding buys: a binary that boots the player on its
  own, offline, double-clicked, or as the installed APK.

The fix removes the smell instead. The screens are apps made by the
JavaScript side, so the JavaScript side ships them, and the library takes
them from the program:

1. **lattice takes a `Program`.** `lattice::main`, `android_main` and
   `android_entry!` take `Program { modules, bsod, player }` (`player` only
   with the `go` feature; the BSOD is in every build because the production
   runtime shows it when a packed app fails to start). The library has no
   `include_str!`, no build.rs and no JavaScript dependency, so it builds
   from any tag.
2. **The stock program is its own crate**, `runtime/` (package `solidrt`):
   lib.rs with `modules()`, `program()` and the `android_entry!` line, the
   two bins, cdylib plus lib, features forwarding to lattice's. It owns
   `resources/` with the make-generated bundles and the build.rs that names
   the make command. The cdylib crate type and the `android-entry` feature
   leave lattice with it: the feature existed only because the library
   doubled as the stock cdylib, and the library never defines an entry
   again. The repo root is itself a project, so its `runtime/` is a custom
   runtime with zero modules, and the shape the scaffold
   ([custom-runtime-authoring](../backlog/custom-runtime-authoring.md))
   copies.
3. **The CLI package ships the bundles.** The publish job bundles the player
   and stages both files into `packages/cli/player/`, as the fonts go into
   `packages/cli/fonts/`: made once by that release's CLI from that
   release's player, so they match the lattice tag of the same version.
4. **A custom runtime embeds them from node_modules** with a relative
   `include_str!`, no variable. The version rule is the one the docs
   already state: lattice tag and `@solidrt` packages at one version.

## Steps

- [x] `Program` in lattice/src/entry.rs, threaded through start, start_with
      and ui_thread in place of bare `Modules`; the consts, the
      `android-entry` line, the cdylib crate type, the bins and build.rs
      out of lattice.
- [x] `runtime/` crate with `resources/` (gitignored as before), a workspace
      member; `lattice/examples/custom_runtime.rs` builds its `Program`
      from `runtime/resources/`.
- [x] lattice/Makefile and the platform files: bundle paths, `-p solidrt`
      on every cargo line, `libsolidrt.so` in the Android staging,
      `android-entry` out of the feature lists. `.cargo/config.toml`,
      `.gitignore`, ci.yml comments.
- [x] release.yml stages the bundles into `packages/cli/player/`; the
      package's `files` list and `.gitignore` carry the folder; the
      version loop covers the new crate.
- [x] docs/runtime "A custom runtime" (the `program()` shape, the
      include_str lines, the bundle rule), apps/player/AGENTS.md,
      CLAUDE.md's project list.
- [x] `make client` and `make runtime` green on Linux, zero warnings, and
      `bun run sol test` green on the new binaries (689 tests, 99 files);
      the Android goals change only the staged file name and are not run.
- [ ] The two custom runtimes outside the repo (tower-toppling, vision)
      move to `Program` once the first release with this is out; until
      then the vision app stays on its vendoring script.

## Done looks like

- A cargo project with only `lattice = { git = ..., tag = "v<version>" }`
  builds both binaries and the Android cdylib, desktop and Android, with
  no vendored tree and no bundling step of its own, its screens from the
  `@solidrt/cli` package at the same version.
- main still tracks no bundle; `runtime/build.rs` still names the make
  command in a checkout.

## Findings

- Bundling is deterministic: two `sol bundle` runs of the player are
  byte-identical. The pre-2026-10-09 churn was generated output following
  source changes, not nondeterminism.
- The BSOD belongs to every build: the production runtime shows it when a
  packed app fails to start (`android-runtime` already depended on the
  bundle), while the player is the dev client's alone.
- Cargo compiles only the library target of a dependency, so moving the
  `include_str!` into the bins would have been enough for the git
  dependency; the crate split is what removes the `android-entry` feature
  and gives the stock program and a custom one the same shape.
