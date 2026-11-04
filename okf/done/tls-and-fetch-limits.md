---
title: TLS for sockets, fetch and WebSocket, and per-host fetch limits
description: One plan over four related items - per-host limits and HTTP/2 for every fetch, a shared forge::tls with TLS sockets in flux:net, TLS options for fetch through a client object, and wss:// - in that order, one stage per item.
created: 2026-11-04
completed: 2026-11-04
---

# TLS for sockets, fetch and WebSocket, and per-host fetch limits

## Problem

Four gaps in the runtime's networking share the same code:

- [fetch-host-limits](fetch-host-limits.md): the per-host cap
  and the 429 cooldown protect only cached fetches, and reqwest has no
  HTTP/2, so a plain fetch flood opens a connection per request.
- [net-tls](net-tls.md): `flux:net` is plain TCP, so an app
  cannot speak or serve a TLS protocol other than HTTPS.
- [fetch-tls-options](fetch-tls-options.md): `fetch` trusts
  the webpki roots only and has no client certificate or pin.
- `wss://`: the WebSocket client rejects it ("no TLS",
  `forge/src/websocket.rs`, `parse_ws_url`).

The backlog items are the specs. This plan fixes the order, the shared
pieces and the decisions taken across them. Each item moves to `done/`
when its stage lands; `wss://` has no item of its own and is specified
here.

## What the stages share

- **`forge::tls`** (built in stage 2): the rustls client and server
  configs from native types (PEM bytes, pin bytes, ALPN strings), the SPKI
  pin `ServerCertVerifier`, the any-key `ClientCertVerifier`, all on the
  ring provider through `builder_with_provider`. Stages 3 and 4 only call
  it.
- **The `tls` forge feature** (stage 2): `forge::tls`, the `flux:net`
  TLS functions and the `"tls"` capability. `http` implies it.
- **The per-host limits** (stage 1): one `HostLimits` per engine, shared
  by every client that engine sends through, the default one and every
  `createHttpClient` client (stage 3).
- **The ALPN lists:** `fetch` offers `h2` and `http/1.1` (stage 1 through
  reqwest, stage 3 set by `forge::fetch` on a preconfigured config).
  `wss://` offers `http/1.1` only (stage 4).
- **The test certificates** (stage 2): PEM fixtures under
  `forge/src/tests/data/tls/`, made once with the `openssl` command-line
  tool and checked in with a long validity: a private CA, a server leaf
  for `localhost` and `127.0.0.1` signed by it, a client leaf signed by
  it, and a self-signed server for the pin cases. The flux tests read the
  same files. The `openssl` commands go in a short README beside them, so
  the set can be made again.

## Decisions taken (2026-11-04)

- One plan, four stages in the order below, each tested and committed on
  its own.
- The `fetch` TLS client object is `createHttpClient(opts)`, an export of
  `flux:http`, used as `fetch(url, { client })`. Deno's name, but not a
  global: a non-standard feature goes behind a `flux:` import.
- A `createHttpClient` client shares the per-host limits with the default
  client. A server sees one app, whatever client it sends through.
- `wss://` joins this plan as stage 4.
- The binary size of h2 is no concern.

## Stage 1: per-host limits for every fetch

Spec: [fetch-host-limits](fetch-host-limits.md).

- `forge/Cargo.toml`: reqwest gains `http2`.
- `forge::fetch`: one request path for `do_fetch` and the network side of
  `do_fetch_cached`, which takes the slot, waits out the cooldown, sends,
  records the response's HTTP version for the host, and retries a 429 on
  a body-less `GET`/`HEAD`. The disk lookup stays before the slot.
  `HostLimits::acquire` skips the semaphore, not the cooldown, for a host
  whose last response was HTTP/2.
- flux: `HostLimits` moves out of the `fetch` closure into engine
  userdata beside the `HttpClient`, so stage 3's clients find the same
  instance.
- `packages/flux-types/standards/fetch.d.ts`: the limits move from the
  `cache` doc to the `fetch` doc, with the HTTP/1.1 trap.
- Tests: `forge/src/tests/fetch.rs` against a local HTTP/1.1 server:
  plain fetches beyond the cap queue, a slot frees when a body ends and
  when its consumer drops it, a 429 with `Retry-After` pauses plain and
  cached fetches alike, a `POST` 429 is not retried. A unit test of
  `HostLimits`: a host recorded as HTTP/2 does not take a slot but does
  wait out a cooldown. The end-to-end HTTP/2 case needs a TLS server and
  lands in stage 3.

## Stage 2: forge::tls and TLS sockets in flux:net

Spec: [net-tls](net-tls.md).

- The fixtures and their README.
- `forge/Cargo.toml`: the `tls` feature; tokio-rustls `=0.26.4`
  (`default-features = false`, `ring`, `tls12`, `logging`) and
  webpki-roots `=1.0.11`, pinned in the workspace table and the member
  manifest.
- `forge::tls`: the configs and the two verifiers. The pin verifier
  checks the SPKI hash, skips the chain and name checks, and delegates the
  handshake signatures to rustls.
- `forge::net`: `Conn` holds an enum per half (TCP, or a half of a split
  TLS stream). A plain TCP `Conn` keeps its read half when the `readable`
  view drops, until `Conn.close()`. `connect_tls`, `start_tls` (takes the
  reader from the slot even with a read pending, ends the old view,
  `reunite`s, upgrades), `listen_tls` (handshakes run concurrently inside
  the listener, each bounded by `HANDSHAKE_TIMEOUT`). `close_write` sends
  `close_notify`; `close` makes one non-blocking attempt at it. A peer
  that closes without `close_notify` ends the read as end-of-stream.
  `alpn_protocol` and `peer_key` on `Conn`.
- flux `forge_plugins/net.rs`: `connectTls`, `startTls`, `listenTls`,
  `alpnProtocol`, `peerKey`; without the feature the three functions throw
  and name it. `"tls"` in the capabilities.
- `packages/flux-types/modules/net.d.ts`, and `docs/runtime/index.md`
  for the new capability name.
- Tests: `forge/src/tests/tls.rs` and `flux/tests/net-tls.test.ts`, the
  list in the spec.

This is the largest stage. The read-half change touches every plain TCP
`Conn`, TLS or not; nothing changes on the wire.

## Stage 3: TLS options for fetch

Spec: [fetch-tls-options](fetch-tls-options.md).

- `forge::fetch::Client` gains a constructor from a `forge::tls` client
  config: `use_preconfigured_tls`, with ALPN `h2` and `http/1.1` set on the
  config.
- flux: `createHttpClient({ caCerts, cert, key, pin })` in `flux:http`
  returns an `HttpClient` with `close()`, which releases its pool. `fetch`
  reads `client` from the init; a closed client rejects the fetch. The
  engine's `HostLimits` serve every client.
- `packages/flux-types/modules/http.d.ts` for `createHttpClient` and
  `HttpClient`, `standards/fetch.d.ts` for `RequestInit.client`.
- Tests (`forge/src/tests/fetch.rs`, `flux/tests/http-client.test.ts`):
  mTLS, a private CA, a pin match and a pin mismatch against a
  local rustls server; and the deferred stage 1 case, an HTTP/2 host that
  takes no slot, against a local HTTP/2 server over TLS (hyper's `http2`
  server feature as a dev-dependency feature only).

## Stage 4: wss:// in the WebSocket client

No backlog item; specified here.

- `parse_ws_url` accepts `wss://` (default port 443) and marks the
  target as TLS.
- `run_client`: after the TCP connect, a TLS handshake with the default
  `forge::tls` client config (webpki roots, the host as server name and
  SNI) and ALPN `http/1.1` only: a server that picked `h2` would answer
  the upgrade with an RFC 8441 extended CONNECT, which this client does
  not speak. Then the hyper handshake and the framing as today.
- A failed TLS handshake reports `error` and then `close`, as a failed TCP
  connect does now.
- `wss://` needs the `tls` feature; the WebSocket client is behind `http`,
  which implies it.
- `packages/flux-types/standards/websocket.d.ts`: `wss://` is supported,
  with the default roots only. The web `WebSocket` constructor has no TLS
  options, so a private CA or a pin is not reachable here; that is stage 3
  of [tls-follow-ups](../plans/tls-follow-ups.md).
- `run_client` takes the `forge::tls` client config as a parameter, and
  the flux plugin passes the default one. So the test certificates reach
  it without a test-only path in the runtime.
- Tests: a forge Rust test that runs `run_client` with the test CA config
  against a local TLS WebSocket server (tokio-rustls in front of the
  existing server side). A JS test cannot do it: the default roots do not
  trust the test CA, and JS has no way to pass another.

## Verification

- `free -g` before each cargo run; one test target at a time, with the
  usual job cap.
- Stage 1: `cargo test -p forge --features http` (the fetch tests).
- Stages 2 to 4: the forge tests with `http` (which implies `tls`), then
  `make -C flux flux` and `bun run sol test` for the flux module tests.
- `cargo test -p flux --lib --features gui` after stage 2 and stage 3,
  for the plugin changes.

## Not in this plan

All but DTLS are in [tls-follow-ups](../plans/tls-follow-ups.md), which
also gives the reason DTLS stays out.

- `startTls` as the server.
- DTLS for `Udp`.
- Certificate generation.
- TLS options for `wss://` (a private CA, a pin, a client certificate),
  for example through a `createHttpClient` client.
- HTTPS serving in `flux:http`.
- Media sources (`forge/src/source.rs`) taking per-host slots.
- One limiter shared across isolates.

## Findings

The one finding, that reqwest's `http2` feature reaches the media source
clients too, is a comment on the reqwest line in `forge/Cargo.toml`. The
deviations of each stage are in the Outcome of its item:
[fetch-host-limits](fetch-host-limits.md), [net-tls](net-tls.md),
[fetch-tls-options](fetch-tls-options.md); `wss://` was built as stage 4
specifies.
