---
title: Put every optional flux module behind a cargo feature
description: A custom runtime build can only leave out what is a cargo feature all the way down, and today that is video, ktx2, wasm-native, speech and test; p2p (iroh), wasm, sqlite, subprocess and mdns are unconditional in forge, flux and the capability list, so a custom player carries them whether it wants them or not; give each optional module the video pattern, with the stock binaries turning them all on.
created: 2026-10-06
---

# Put every optional flux module behind a cargo feature

## Symptom

The custom runtime route
([runtime-extension-modules](runtime-extension-modules.md)) works by
addition and by subtraction: a project depending on lattice as a crate
lists the features it wants, `default-features = false`, and anything not
listed is neither compiled nor linked. Subtraction only works for a module
that is a cargo feature at every layer: the forge dependency, the forge
module, the flux plugin registration and the entry in
`BASE_CAPABILITIES`. Today that holds for `video`, `ktx2`, `wasm-native`,
`speech` and `test`.

It does not hold for the rest. `iroh` is an unconditional dependency in
`forge/Cargo.toml`, `flux:p2p` is registered unconditionally in
`flux/src/plugins/mod.rs`, and `"p2p"` is a fixed capability. The same is
true of `flux:wasm` (only the lane is a feature; wasmi is always compiled),
`flux:sqlite`, `flux:subprocess` and `flux:mdns`. A custom player that
has no use for peer-to-peer gets iroh and its TLS and QUIC stack anyway.

The size work in [runtime-optimization](runtime-optimization.md) names the
p2p stack and sqlite among the unmeasured weights; a feature per module is
also what makes those measurable, one build with and one without.

## Shape

The `video` pattern, three edits per module:

- **forge** owns the dependency: `p2p = ["dep:iroh"]`, with `pub mod p2p`
  and the iroh-only dependencies behind `cfg(feature = "p2p")`. A feature
  that gates the module but not the dependency shrinks nothing.
- **flux** passes it through: `p2p = ["forge/p2p"]`, gating the plugin
  module, the `add_module` lines and the capability entry, so
  `Flux.capabilities` tells the truth for a build without it.
- **lattice** passes it through: `p2p = ["flux/p2p"]`. Where lattice's own
  code uses the module, the owning feature depends on it: `go` needs `p2p`
  for the dev client's tunnel and connection under `lattice/src/go/`.

The stock Makefile goals turn every module on, so the shipped runtime and
player do not change. The candidates, by what they pull in:

| module | dependency the feature would own |
|---|---|
| `flux:p2p` | iroh and its network stack |
| `flux:wasm` | wasmi (the lane feature already owns wasmtime) |
| `flux:sqlite` | the sqlite binding |
| `flux:subprocess` | the process core |
| `flux:mdns` | the DNS codec (shared with iroh's deps while p2p is on) |

`fs`, `http`, `path`, `process`, `isolate`, `image`, `svg` and `tty` stay
unconditional: they are the foundation an app and the runtime itself stand
on, not optional capabilities.

## Done looks like

- Each module in the table is a feature in forge, flux and lattice, and a
  lattice build with `--no-default-features --features flux/compile` links
  none of their dependencies.
- The stock `make client` and `make runtime` output is unchanged.
- CI compiles one build with the features off, so a cfg break reaches no
  release (the `ktx2` line in tiny.md is the same gap).
- The size table in runtime-optimization gets one row per module from the
  with-and-without builds.

## Not in scope

- Changing what the stock binaries ship. Which modules a default runtime
  carries is a product decision the size numbers inform; this item only
  makes the choice possible.
- Feature-gating the gui plugins (`alloy_plugins/`): lattice is the
  renderer, and a headless flux without `gui` already exists.
