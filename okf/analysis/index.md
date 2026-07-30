---
type: bundle-index
title: Analysis
description: Point-in-time assessments of the codebase; one file per analysis, dated in frontmatter.
timestamp: 2026-07-15T00:00:00Z
---

# Analysis

- [App structure and performance](app-structure-performance.md) -
  Execution-structure review: the JS-decides/Rust-executes split is right;
  ranked costs are per-prop string-keyed FFI writes, per-frame JS animation,
  JS scroll physics, event garbage.
- [GPU stack review](gpu-review.md) - Merged status and direction: retro-class
  3D feasible, first-person still blocked on mouse look; the object model is
  WebGPU-adjacent with one deep divergence - a retained pure-target model
  whose purity question gates accumulation, feedback and multi-pass. Ranked
  lessons from WebGL2/WebGPU, capability gaps by workload, and a file split
  proposal.
- [Forge crate review](forge-crate-review.md) - Engine-free layering upheld,
  docs excellent, clippy clean; gaps are untested subprocess/p2p/ffi, stale
  docs, an implicit single-thread contract and IPv4-only skew.
- [Alloy crate review](alloy-crate-review.md) - The GL path is complete and
  hardened; gaps are unenforced unsafe Send/Sync, panics at the tree boundary,
  and thin tests with damage and hit testing uncovered.
- [Flux crate review](flux-crate-review.md) - Marshalling contract upheld,
  error model strong, 129 tests pass; gaps span gui prop panics, fetch
  dropping Headers, standards conformance, and missing teaching examples.
- [Core package review](core-package-review.md) - Best docs and layering in
  the repo; gaps are zero tests, docs teaching nonexistent props, a throwing
  onFrame killing sibling animations, and silently black invalid colors.
- [CLI package review](cli-package-review.md) - Architecture strong and
  dogfoods flux as the dev server; gaps are zero tests, a crashing --help,
  stale README, tsx-only repl reload, watcher races and a LAN-open file PUT.
