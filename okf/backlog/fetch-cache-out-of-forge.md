---
title: Move the fetch disk cache out of forge?
description: Lattice is now the only cache configurer, so should the mechanism follow the policy out of forge, and which of the three candidate shapes pays for itself?
created: 2026-07-24
---

# Move the fetch disk cache out of forge?

Follow-up to the 2026-07-24 storage cleanup (project-local `.srt-data`
reverted, `dev_cache_dir` deleted, bare flux/fluxrt bins cache-free).
That change established that caching is embedder policy, not runtime
behavior: after it, the only thing that ever configures a cache dir is
lattice. The open question is whether the cache MECHANISM should follow
the policy upward - it reads as a lattice (app-runtime) concern, while
forge is the capability foundation.

What lives where today:

- `forge/src/cache.rs` - the disk store (LRU, size cap, blake3 file
  names, scan for browsing).
- `forge/src/fetch.rs` - `do_fetch_cached` + `CacheMode` +
  `cached_meta`, and `HostLimits` (per-host cap + 429 host cooldown,
  asset-mode only) which is entangled with the cached path.
- flux `standards/fetch.rs` - the `cache` fetch option; builds the
  `Cache` from the injected `FetchCacheDir` userdata. Without a dir the
  option degrades to plain network.
- lattice - the only configurer (per-app dir from storage resolution)
  and the launcher browse/clear consumer (`forge::cache::scan` in
  go/store.rs).

The wrinkle: flux cannot call into lattice (flux depends on forge, not
the other way around), so "move it to lattice" cannot be a simple crate
move while the `cache` option stays part of flux's fetch surface. The
real shapes to weigh:

1. Status quo: forge hosts the generic mechanism, everything above
   injects only policy (a directory). Defensible under "generic inside,
   fetch-only outside", but forge carries code with exactly one real
   consumer.
2. Widen the injection point: flux's fetch accepts a cache
   implementation (trait object) instead of a directory; the store and
   the 429/host-limit logic move to lattice. Bare flux stays truly
   cache-free in code, not just in configuration. Cost: a trait
   boundary through the streaming write-through path, and the `cache`
   option's semantics now vary by embedder.
3. Take the `cache` option out of standard fetch entirely and make
   caching a lattice-side fetch wrapper. Cleanest layering, but breaks
   the "standard vocabulary" contract (`cache` is a standard
   RequestInit member) and forks fetch behavior between bare flux and
   GUI apps at the API surface.

Related tension to resolve with it: the 429 backoff currently protects
cached (asset) fetches only; if the cache moves, decide where that
protection lives. Authoritative history: okf/plans/fetch-cache.md.
