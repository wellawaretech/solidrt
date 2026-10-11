---
title: Make a lattice git dependency build at a release tag
description: A custom runtime depends on lattice by git tag, but since v0.0.71 a tag carries no player or BSOD bundle (untracked because every make client rewrote them) and lattice/build.rs panics, so the documented dependency cannot build and a custom runtime has to vendor the tag and bundle the player itself; the release should make a tag that carries the bundles, leaving main untracked.
created: 2026-10-11
---

# Make a lattice git dependency build at a release tag

## Symptom

docs/runtime ("A custom runtime") has a custom runtime depend on lattice by
git tag. lattice embeds two generated bundles, `resources/player/index.sol.js`
and `resources/bsod/bsod.sol.js`, made from `apps/player` by `make -C
lattice player-bundle`. They were untracked on 2026-10-09 (0284fadf),
because `make client` rewrote the player bundle by a few lines on every
build. Since then `lattice/build.rs` panics when they are missing, which is
right for a checkout and fatal for a git dependency: cargo's checkout of the
tag has no `node_modules` and no bun step, so it cannot make them. v0.0.71
and v0.0.72 are both affected; up to v0.0.70 the tags still carried them.

Today's way around it costs a custom runtime project a script and two
traps:

- Export the tag with `git archive`, add the submodules, bundle the player
  and BSOD from the tag's `apps/player` with the app's own CLI, and depend
  on lattice by path.
- A path dependency inside the project's folder joins its workspace, so
  lattice's `workspace = true` keys resolve against the project's manifest;
  the project needs `exclude = [...]` for the vendored tree.
- A path dependency does not use SolidRT's `Cargo.lock`, so the project
  copies the tag's lockfile in to keep the stock crate versions. They have
  to match: the derived APK swaps only `libmain.so`, so `libSDL3.so` and
  `libimpeller.so` come from the stock build.

## Options

- **A release tag that carries the bundles (recommended).** After the
  release workflow bundles the player, it commits the two bundles
  (force-added) on top of the tagged commit and pushes them as a second
  tag, `lattice-v<version>`. That name does not match the workflow's
  `v*` trigger, so it starts no second release. main stays untracked and
  determinism does not matter, since the tag is made once. The docs'
  dependency changes only its tag. Costs: the workflow needs `contents:
  write` for that one step, and a dry run skips it.
- **Track the bundles again.** Make the bundling reproducible (find the
  lines that change per build), track them, and have CI fail when they are
  stale (`make -C lattice player-bundle`, then `git diff --exit-code
  lattice/resources`). Every core, components or player change then
  carries a bundle diff and a rebundle before commit: the state before
  2026-10-09, now enforced.
- **The bundles from the published packages.** Stage them into
  `@solidrt/cli` at release (as the fonts are), and have `build.rs` read
  them from a directory named by an environment variable. A custom runtime
  build sets it to its `node_modules`. One more variable for every custom
  runtime build, and the lattice tag and the CLI version can disagree.

## Done looks like

- A cargo project with only `lattice = { git = ..., tag = <the release's
  lattice tag> }` builds both binaries and the Android cdylib, desktop and
  Android, with no vendored tree and no bundling step of its own.
- docs/runtime shows that dependency, and says which tag goes with which
  `@solidrt` version.
- main still tracks no bundle, and `build.rs` still names the make command
  in a checkout.
