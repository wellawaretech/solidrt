---
type: backlog-item
title: Generic home for cross-crate constants and configuration
status: deferred
timestamp: 2026-07-17T00:00:00Z
---

# Generic home for cross-crate constants and configuration

SolidRT-wide names and knobs are currently per-site literals with no shared
definition, and some exist twice across the Rust/TypeScript boundary. A few
that exist today:

- `.srt-data` - the project-local dev data root (flux
  `FluxEngineBuilder::dev_cache_dir`, future data-root resolution, CLI).
- `.srt-cache.db` - the dev server proxy cache file (`packages/cli`).
- `SolidRT` / `go` - the generic client's pref-path identity (lattice).
- The fetch cache size cap (flux, placeholder 256 MB).

Wanted: one generic place to define these - possibly a config file the
runtime reads, possibly a shared constants module per language with a parity
rule like flux-types. This is a larger design question (what is compile-time
constant vs runtime configuration, and who may override what); do not solve
it piecemeal by scattering `pub const` at crate roots. Until then new sites
keep the literal local and this note collects them.