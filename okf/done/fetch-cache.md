---
title: Fetch disk cache
description: Explicit opt-in caching in the forge fetch core (server cache headers ignored) with a per-app store and an LRU size cap, then GET coalescing and per-host limits.
created: 2026-07-17
completed: 2026-07-17
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
  which also fixes the postmortem's hot-reload refetch problem without
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
update-mechanism data roots land; the literals are collected in
okf/backlog/shared-config-constants.md. Update 2026-07-24: the cwd
`.srt-data/cache` default scattered folders across the filesystem and put
storage policy inside the engine; `dev_cache_dir()` is deleted and the
bare flux/fluxrt bins configure no cache dir at all - a scripting runtime
silently persisting responses to disk is hidden magic, and the documented
off state (`cache` accepted, every request hits the network) covers them.
Only embedders opt in via the plain `cache_dir` builder.

**Stage 2 - politeness moves down, core shrinks.** DONE 2026-07-17, with one
scope change: GET coalescing was dropped (see named futures below). A
per-host concurrency limit for cached fetches lives in the fetch layer
(`HostLimits` in `forge/src/fetch.rs`: async RAII permits, the permit rides
the response body stream so it releases when the body completes or is
dropped; disk hits bypass it; plain `do_fetch` is deliberately never
throttled - API calls, long-polls, and streams must not queue behind asset
traffic). `packages/core/src/image.ts` deleted its 4-slot fetch gate and
session failure cache, keeps `decodeImage` plus the refcounted URL ->
texture map (decode+upload dedupe is the irreducible JS part - a byte cache
cannot provide texture sharing), and `createImage` passes `force-cache`
(images are assets; the layer that knows chooses the policy). Behavior
change, deliberate: a failed URL now retries on remount instead of staying
failed for the session (recovers with the network; the per-host cap bounds
the damage). E2e verified: cached peak = limit, plain fetches unthrottled,
disk hits bypass.

**Deferred until a need shows up:** cap-size knob, opt-in `maxAge`,
revalidation.

**Named futures** (recorded so later work does not invent vocabulary):

- In-flight GET coalescing (two concurrent cached fetches of one URL share
  one network request). Dropped from stage 2: core's refcounted image map
  already coalesces the motivating path (mounts of one URL share one fetch),
  and a naive fetch-layer coalescer that waits on the first request's cache
  commit would make one caller's fetch hang on another caller's body
  consumption. The correct shape is browser-style response sharing - a
  waiter streams the in-progress entry file as it grows - which is real
  machinery to build when a consumer without its own dedupe map needs it.

- Cache management - clear, inspect size, evict one URL. Real even with the
  cache internal; likely surfaces as a dev/MCP concern first. Does not
  require exposing storage.
- If a public cache surface is ever warranted after all, the shape is the
  web Cache API (`caches.open()`, `cache.match/put` - request/response
  keyed, explicitly fetch-adjacent), not a bespoke KV API. The standalone
  forge cache core is what keeps that cheap to add.

## 2026-07-23 additions (cheezed 429 debugging)

Driven by an app loading ~185 remote images: rate-limited hosts
(upload.wikimedia.org) returned 429s that surfaced as broken images, and the
client-level shared cache could not be inspected or cleared per app.

- **Reactive 429 backoff, asset mode only** (`do_fetch_cached`): a 429 puts
  the whole host on cooldown - `HostLimits` gained an extend-only
  `cooldown(host, delay)`, and `acquire` sleeps out the cooldown while
  already holding its permit, so a 429'd host drains idle instead of freed
  slots re-flooding it. Delay: `Retry-After` when sent (delta-seconds; over
  60s means give up and return the 429), otherwise full-jitter exponential
  (uniform in [0, 500ms * 2^attempt], fastrand). 3 retries; requests with a
  body never retry (streams cannot be replayed). Decided AGAINST: proactive
  pacing/token buckets (no known quota to encode) and first-attempt jitter
  (that is fleet-splay, not a single-client concern); plain `do_fetch`
  stays policy-free.
- **Per-app cache dirs**: `Storage::cache_dir(app_id)` = `app_dir/cache`
  (packed flat layout unchanged); the lattice engine loop resolves the dir
  per iteration from the anchored app id. An app's cache dies with it on
  remove; the size cap is now per app. Old client-level dirs are abandoned
  without migration (project is pre-release).
- **Cache management delivered** (was a named future): `cache::scan` in the
  forge core (sync; `read_header` unified sync, `lookup` converts the same
  handle via `File::from_std` for the streamed body) + `fetch::cached_meta`
  (url + normalized content-type). `srt:apps` `info()` gained `cacheSize` +
  `cache: [{url, type?, size}]` and a `clearCache(id)`; the launcher detail
  view aggregates by content type and by domain with a clear button. The
  JS surface stays the flat per-entry primitive; grouping is view-side.
- **User-Agent**: flux default is now `FluxRT/<FLUX_VERSION>` (git-describe
  stamp, not the 0.0.0 Cargo placeholder); `FluxEngineBuilder::user_agent`
  lets the embedder substitute its identity, lattice sends
  `SolidRT/<VERSION>`. Layering: no top-level product names in flux.

## Open questions

- ~~Cache directory before the update-mechanism data roots exist~~ resolved
  in stage 1, revised 2026-07-24: bare flux scripts have no disk store
  (the engine only takes an explicit `cache_dir`; no embedder, no cache),
  lattice uses the generic client's SDL pref path (`SolidRT/go/cache`,
  resolution rule 3 in the update-mechanism research). Relocating a cache
  is free.
- Mobile cache placement (noted 2026-07-17). On Android the SDL pref path is
  the internal *files* dir, so the cache is reported as app data: the user's
  "Clear cache" cannot remove it and the OS cannot reclaim it under storage
  pressure. It belongs in the platform cache dir, which SDL already
  exposes as `SDL_GetAndroidCachePath()` (bound by sdl3-sys, unwrapped by
  the safe sdl3 crate, so one line in sdl_utils - no JNI hop, contrary to
  an earlier revision of this note). iOS analog:
  purgeable content belongs in `Library/Caches` (excluded from backup), not
  `Application Support`. Robustness is already covered - entries are
  self-contained files and OS deletion at any moment just means a miss - so
  this is purely placement, and moving a cache costs nothing. Fold into the
  update-mechanism data-root resolution (its tree already separates
  `cache/` from app data).
- Whether `--proxy-http`'s dev-server cache keeps its own store or delegates
  once this exists (revisit after stage 1; keep it as-is meanwhile).
- Exact default for the size cap (256 MB is a placeholder number, a const in
  the flux fetch plugin; see also okf/backlog/shared-config-constants.md).
- Per-app dirs wart (2026-07-23): returning to the launcher (EngineCmd::Stop
  carries no app id) leaves the launcher engine on the previous app's cache
  dir. Harmless while the launcher does no cached fetches; give the
  launcher a fixed id if that changes.