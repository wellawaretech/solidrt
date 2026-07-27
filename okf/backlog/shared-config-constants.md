---
type: backlog-item
title: Home for cross-crate constants
description: One defined home for cross-crate constants that today live as per-site literals (.srt-data, http-cache.db, the SolidRT/go identity, size caps); collects sites until designed.
status: deferred
timestamp: 2026-07-27T00:00:00Z
---

# Home for cross-crate constants

SolidRT-wide names and knobs are currently per-site literals with no shared
definition, and some exist twice across the Rust/TypeScript boundary. A few
that exist today:

- `.srt-data/http-cache.db` - the dev server proxy cache file
  (`packages/cli`).
- `SolidRT` / `go` - the generic client's pref-path identity (lattice).
- The fetch cache size cap (flux, placeholder 256 MB).
- The per-host concurrency limit for cached fetches (flux,
  `FETCHES_PER_HOST`).
- The numbered client dir shape `client<N>` under the pref root
  (lattice, `storage.rs`).
- The app icon size cap (`ICON_MAX_BYTES`, lattice `go/store.rs`).

Convention until then: the value lives in exactly ONE code constant; docs
and JSDoc describe the behavior without repeating the number, so making a
knob configurable later means touching one site.

Wanted: one generic place to define these - possibly a config file the
runtime reads, possibly a shared constants module per language with a parity
rule like flux-types. This is a larger design question (what is compile-time
constant vs runtime configuration, and who may override what); do not solve
it piecemeal by scattering `pub const` at crate roots. Until then new sites
keep the literal local and this note collects them.