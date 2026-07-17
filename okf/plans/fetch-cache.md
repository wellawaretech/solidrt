---
type: plan
title: Fetch disk cache (explicit opt-in, forge fetch core)
status: stage 1 done
timestamp: 2026-07-17T00:00:00Z
---

# Fetch disk cache

Persistent, general-purpose caching for `fetch()`, decided 2026-07-17 out of
the image review (core's `createImage` grew an in-memory, image-only loading
subsystem; the byte-caching half of it belongs below JS and should not be
image-specific). Supersedes the cache-control-honoring framing in
`okf/backlog/engine-http-cache.md`.

## The model (decided)

**App runtime, not browser.** A browser needs header-driven caching because it
cannot know what any resource is; a solidrt app targets known resources, so
the policy lives at the call site. Precedent: Node, Bun, and Deno `fetch`
have no HTTP cache at all - that is the expected default for a non-browser
runtime.

- `fetch()` by default: no caching, ever. Network every time. API traffic
  stays predictable with zero configuration.
- `cache: "force-cache"`: serve from disk if present, otherwise fetch and
  store. No freshness, no TTL: an entry lives until evicted by the size cap.
  This is the asset mode (images, audio, fonts).
- `cache: "reload"`: fetch fresh and overwrite the stored entry. The explicit
  "I know it changed" escape hatch; versioned URLs are the normal way apps
  handle updatable assets.

Standard option names (an LLM or web developer guesses them without reading
docs), deliberately simplified semantics, documented where agents already
look: JSDoc on the `cache` option in `packages/flux-types/standards/fetch.d.ts`
plus `docs/flux.md`.

Other standard `cache` vocabulary (`"default"`, `"no-store"`, `"no-cache"`)
is accepted as a no-op (all mean "just hit the network" here); unrecognized
values follow the validation policy (throw in dev, warn in prod - see
`okf/backlog/dev-prod-validation-policy.md`; today that means throw).

Deliberate non-goals, to state in the docs as "what this is not":

- Server cache headers are ignored entirely (`cache-control`, `expires`,
  `etag`). The developer outranks the server in an app.
- No freshness model, no heuristic TTL, no conditional revalidation, no
  `Vary`. An unversioned URL cached with `force-cache` never updates until
  evicted or `reload`ed - that is the contract, one sentence.
- Only GET requests with 2xx responses are cached. On non-GET requests the
  `cache` option is ignored (documented, matches browser behavior).
- No cross-app shared cache.
- No public cache API (decided 2026-07-17). The public surface is the fetch
  `cache` option, nothing else. Rationale: everything cacheable in this
  runtime arrives by URL through fetch (models, fonts, audio, images), so
  the fetch option already serves every real consumer; and a cache's
  contract is "may vanish at any moment" (LRU) - a public put/get would
  inevitably get used as durable storage and lose data on eviction. Keeping
  the surface fetch-only makes the eviction contract unbreakable by
  construction: the cache can only hold refetchable things, because the URL
  is the key. Apps that want to persist computed bytes want a store, not a
  cache: flux:sqlite / flux:file.

## Implementation shape

- Structured as a standalone cache core in forge (store, lookup, evict;
  knows nothing about HTTP), with fetch as its first consumer - the house
  pattern (engine-free cores, consumers plug in). Generic on the inside,
  fetch-only on the outside.
- Wired into the forge fetch core so every consumer benefits: headless flux,
  lattice, and everything above ("images only" restriction rejected).
- Store: one file per entry keyed by URL hash, small metadata header (URL,
  status, response headers), in the per-app data root (`.srt-data` in dev -
  which also fixes the kaas hot-reload refetch problem without
  `--proxy-http`; the prod org/app path once the update-mechanism data roots
  land). No sqlite, no index file.
- Eviction: LRU by file mtime, fixed size cap (default 256 MB) enforced
  lazily on write, plus a per-entry cap so one giant download cannot evict
  everything.
- Streaming: response bodies stream, so the cache writes through as chunks
  arrive and commits the entry only on clean body completion.

## Staging

**Stage 1 - the cache itself.** DONE 2026-07-17. Disk store in forge
(`forge/src/cache.rs`: opaque-metadata entries, blake3-of-key file names,
tee-to-temp-file with commit on clean completion, mtime LRU + per-entry cap;
`do_fetch_cached` in `forge/src/fetch.rs`); `cache` option threaded through
the flux fetch plugin (unknown values throw per the dev/prod validation
policy); `FluxEngineBuilder::cache_dir` + `dev_cache_dir()` (cwd
`.srt-data/cache`, used by the flux/fluxrt bins); lattice passes SDL
`get_pref_path("SolidRT", "go")/cache`. flux-types + docs/flux.md updated;
forge unit tests + e2e smoke (modes, throw, POST ignored, cross-restart
disk hit) verified. Interim locations resolved this way until the
update-mechanism data roots land; the `SolidRT`/`go`/`.srt-data` literals
are collected in okf/backlog/shared-config-constants.md.

**Stage 2 - politeness moves down, core shrinks.** In-flight GET coalescing
(two concurrent fetches of one URL share one network request) and a per-host
concurrency limit in the same fetch layer. Then `packages/core/src/image.ts`
deletes its 4-slot fetch gate and session failure cache, keeps only
`decodeImage` plus the refcounted URL -> texture map (decode+upload dedupe is
the irreducible JS part - a byte cache cannot provide texture sharing), and
`createImage` passes `force-cache` by default (images are assets; the layer
that knows chooses the policy).

**Deferred until a need shows up:** cap-size knob, opt-in `maxAge`,
revalidation.

**Named futures** (recorded so later work does not invent vocabulary):

- Cache management - clear, inspect size, evict one URL. Real even with the
  cache internal; likely surfaces as a dev/MCP concern first. Does not
  require exposing storage.
- If a public cache surface is ever warranted after all, the shape is the
  web Cache API (`caches.open()`, `cache.match/put` - request/response
  keyed, explicitly fetch-adjacent), not a bespoke KV API. The standalone
  forge cache core is what keeps that cheap to add.

## Open questions

- ~~Cache directory before the update-mechanism data roots exist~~ resolved
  in stage 1: flux scripts use cwd `.srt-data/cache`
  (`FluxEngineBuilder::dev_cache_dir`), lattice uses the generic client's
  SDL pref path (`SolidRT/go/cache`, resolution rule 3 in the
  update-mechanism research). Both interim; relocating a cache is free.
- Whether `--proxy-http`'s dev-server cache keeps its own store or delegates
  once this exists (revisit after stage 1; keep it as-is meanwhile).
- Exact default for the size cap (256 MB is a placeholder number, a const in
  the flux fetch plugin; see also okf/backlog/shared-config-constants.md).