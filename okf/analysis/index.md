---
type: bundle-index
title: Analysis
description: Point-in-time assessments of the codebase; one file per analysis, dated in frontmatter.
timestamp: 2026-07-15T00:00:00Z
---

# Analysis

- [GPU stack maturity — readiness for 3D games](gpu-stack-maturity.md) -
  vertex pipelines shipped 2026-07-15; retro-class 3D feasible now; gaps:
  typed uniforms, index buffers, blending, multi-pass, sampling control,
  mouse look.
- [Forge crate review - completeness, quality, tests](forge-crate-review.md) -
  full-crate review 2026-07-15; engine-free layering upheld, docs excellent,
  clippy clean; gaps: subprocess/p2p/ffi untested, stale "destined for forge"
  docs, implicit single-thread contract, IPv4-only skew.
- [Alloy crate review - completeness, quality, tests](alloy-crate-review.md) -
  full-crate review 2026-07-15; GL path complete and hardened, docs
  excellent; gaps: unsafe Send/Sync unenforced, panics at tree boundary,
  thin tests (damage + hit testing uncovered).
- [Flux crate review - completeness, quality, tests](flux-crate-review.md) -
  full-crate review 2026-07-15; marshalling contract upheld, error model
  strong, 129 tests pass; gaps: gui prop-value panics, gui_hello breaks
  cargo test, fetch drops Headers instances, 1ms idle poll, no URL global,
  subprocess/p2p/wasm/ffi/gui untested; standards-conformance audit (SDL key
  names not W3C, repeat dropped, console Error prints {}); flux-types near
  complete (atob/btoa missing, 2 doc defects, no parity check); no teaching
  examples (core has 20, flux has smoke scripts).
- [Core package review - completeness, quality, tests](core-package-review.md) -
  full-package review 2026-07-15; best docs and layering in the repo, createX
  lifecycle pattern consistent; gaps: zero tests, docs/core.md teaches
  nonexistent props, throwing onFrame kills sibling animations, KeyEvent has
  no modifiers, invalid colors silently black.
- [CLI package review - completeness, quality, tests](cli-package-review.md) -
  full-package review 2026-07-15; architecture and comments strong, dogfoods
  flux as dev server; gaps: zero tests, --help crashes with a stack trace,
  stale README, repl reload/load are .tsx-only, watcher race + unguarded
  rejections, LAN-open file PUT, dead bonjour-service dep.
