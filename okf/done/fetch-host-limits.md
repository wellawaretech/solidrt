---
title: Per-host limits for every fetch
description: The per-host concurrency cap and the 429 cooldown protect only cached fetches, so a plain fetch flood opens a connection per request and gets an app rate-limited; apply them to every fetch the way browsers cap HTTP/1.1, and turn on HTTP/2 so a multiplexing host skips the cap.
created: 2026-11-04
completed: 2026-11-04
---

# Per-host limits for every fetch

## Problem

`fetch` has two protections, and both are tied to the cache mode
(`do_fetch_cached` in `forge/src/fetch.rs`):

- `HostLimits` caps each host at `FETCHES_PER_HOST` (6) requests in
  flight. A permit rides the response body and is released when the body
  ends or its consumer drops it.
- A 429 puts the host on a cooldown (`Retry-After`, or jittered
  exponential backoff), and the request retries up to `RETRY_LIMIT`
  times.

Only `cache: "force-cache"` and `cache: "reload"` take that path. So
`createImage` is protected, and the same 100 images fetched with plain
`fetch` are not. reqwest pools connections but caps nothing: with no
limit per host and no HTTP/2 (forge builds reqwest without the `http2`
feature), 100 parallel requests open 100 TCP and TLS connections to one
host. That burst is what gets an app rate-limited, and then nothing
backs off.

The coupling is also wrong in itself: how much an app may load from a
host at once has nothing to do with whether the bytes go to the disk
cache. The current reason for the exemption, that long-polls and streams
must not queue behind assets, is the HTTP/1.1 trade-off that browsers
make for every request.

## Done looks like

- Every `fetch` to a host takes a slot from that host's limit, cached or
  not, whatever the method. The limit and its key (host plus effective
  port) stay as they are. A disk hit still takes no slot.
- reqwest is built with `http2`. A host whose last response came over
  HTTP/2 skips the cap: its requests share one connection, and the
  server's `SETTINGS_MAX_CONCURRENT_STREAMS` is the limit. A host is
  capped until its first response says otherwise, so the first burst to
  a new host opens at most `FETCHES_PER_HOST` connections either way.
- A 429 from any fetch puts the host on its cooldown, and every request
  to that host waits it out before it starts, on HTTP/1.1 and HTTP/2
  alike.
- A body-less `GET` or `HEAD` retries a 429 as cached fetches do today.
  Any other request hands the 429 to the caller: a request with a body
  cannot be replayed, and a non-idempotent method is the caller's
  decision to repeat.
- The `cache` doc in `packages/flux-types/standards/fetch.d.ts` loses
  its "plain fetches are never throttled" sentence. The limits move to
  the `fetch` doc itself, with the HTTP/1.1 trap in plain words: a
  request holds its slot until its body ends, so `FETCHES_PER_HOST`
  long-polls, event streams or long downloads to one HTTP/1.1 host hold
  up every other request to it. A plain `http://` host is always
  HTTP/1.1. The way out is a WebSocket, which takes no slot.
- The comment in `packages/core/src/image.ts` that credits the fetch
  layer with the per-host limit still holds; check its wording.
- Tests against a local server: plain fetches beyond the cap queue; a
  slot frees when a body ends and when its consumer cancels it; a 429
  with `Retry-After` pauses plain and cached fetches to that host; a
  `POST` 429 is not retried; an HTTP/2 host is not capped.

## Involves

**The browser model, and where it departs.** Browsers cap HTTP/1.1 at 6
connections per host for every request and multiplex HTTP/2 without a
cap. The cap and the HTTP/2 exemption follow them. The 429 cooldown and
retry do not come from browsers, which return a 429 to the page and do
nothing else. They stay because a runtime that runs one known app
against its own servers should not let one image flood lock the app out
of its API, and because a 429 is the server asking for exactly this
pause.

**forge.** `HostLimits` comes out of the cached path and into the
request path that both `do_fetch` and `do_fetch_cached` use. The cached
path keeps the disk lookup before the slot is taken. The retry loop
moves with it, gated on body-less `GET`/`HEAD`. `ResponseData` gains the
negotiated HTTP version, and `HostLimits` records it per host as each
response arrives, so `acquire` can skip the semaphore (but not the
cooldown) for an HTTP/2 host. The last response wins: a host that moves
back to HTTP/1.1 is capped again from its next request on.

The permit is taken before the request is sent, so there is a race: the
second burst to a host can start before the first response has
recorded HTTP/2. The cap then applies once more, which is harmless.

**reqwest.** Add `http2` to reqwest's features in `forge/Cargo.toml`
(h2 0.4.15 is in the lockfile already). reqwest offers `h2` and `http/1.1` by ALPN itself, as long as it builds
the rustls config. [fetch-tls-options](fetch-tls-options.md) hands it a
preconfigured config, and then `forge::fetch` sets the same two
protocols.

**Placement.** The limiter stays in `forge::fetch`, next to the client
it limits. Moving it to lattice with the cache is the question of
[fetch-cache-out-of-forge](fetch-cache-out-of-forge.md); this item takes
the limiter out of that question, because it no longer belongs to the
cache.

## Out of scope, related

- Media sources (`forge/src/source.rs`) build their own reqwest clients
  per user agent and take no slots. Whether a remote video or audio
  stream should count against its host's limit is a separate decision:
  it is the long-lived request the HTTP/1.1 trap is about. That decision
  is stage 5 of [tls-follow-ups](../plans/tls-follow-ups.md).
- The limiter is per engine. An app that fetches from several isolates
  gets a cap per isolate, where a browser profile shares one cap across
  all its tabs. One limiter per app is stage 4 of
  [tls-follow-ups](../plans/tls-follow-ups.md).
- The WebSocket client opens its own connections and takes no slot, as
  in browsers.

## Outcome

Stage 1 of [tls-and-fetch-limits](tls-and-fetch-limits.md).
Built as shaped, with these choices:

- `do_fetch` is the limited path and takes the engine's `HostLimits`;
  the raw request is `forge::fetch::send`. The lattice go dev-server
  proxy sends through `send`: it funnels every host into the dev
  server, so a cap there would put the whole app on 6 slots to one
  local HTTP/1.1 server.
- Every 429 sets the cooldown, also the last one of an exhausted retry
  and one on a request that does not retry. A `Retry-After` above
  `RETRY_AFTER_MAX` still sets none: the caller gets the 429.
- The end-to-end "an HTTP/2 host is not capped" test needs a TLS server
  and is stage 3's; a `HostLimits` unit test covers the rule here.
