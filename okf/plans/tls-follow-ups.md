---
title: TLS follow-ups - HTTPS serving, server startTls, certificates, WebSocket options, one limiter per app
description: The gaps tls-and-fetch-limits left "until a real app needs it" - HTTPS (and h2) in serve, startTls as the server, certificate generation, subprotocols and a TLS client for WebSocket, the per-host limits shared across isolates - plus the open decision on media sources and the cap; one stage each.
created: 2026-11-05
---

# TLS follow-ups

## Problem

[tls-and-fetch-limits](../done/tls-and-fetch-limits.md) left these out
"until a real app needs it". solidrt aims to support any app, and each one
is something Deno, Bun, Node or the web platform ships:

- `serve` in `flux:http` speaks plain HTTP only. `Deno.serve` and
  `Bun.serve` both serve HTTPS.
- `startTls` in `flux:net` upgrades as the client only, so an app cannot
  serve SMTP, IMAP or XMPP `STARTTLS`. Node does it with
  `new tls.TLSSocket(socket, { isServer: true })`.
- An app cannot make a certificate. `listenTls` (and HTTPS serving)
  need PEM the app brings, and a LAN app that pairs by pin needs an
  identity of its own on first run.
- `wss://` trusts the web roots only, so a server that `fetch` reaches
  through a `createHttpClient` client is out of reach of `WebSocket`. The
  constructor also takes no subprotocols, the standard second argument.
- The per-host limits are per engine. An app that fetches from several
  isolates gets a cap per isolate, against the decision that a server sees
  one app.
- Media sources (`forge/src/source.rs`) take no per-host slot. This one
  is an open design question, not a gap: see stage 5.

## Decisions taken (2026-11-05)

- One plan, five stages in the order below, each tested on its own.
- DTLS for `Udp` stays out: Node, Deno, Bun and the web expose none,
  rustls has none, and it would add a second TLS stack (OpenSSL) beside
  rustls.
- Certificates come from rcgen `=0.14.10` (the rustls project, on the ring
  provider as `forge::tls` is), behind the `tls` feature.
- The server role of `startTls` is an `isServer: true` option, Node's
  name, not a second function.
- HTTPS serving offers `h2` and `http/1.1` by ALPN, as `Deno.serve` does.
  The WebSocket upgrade stays HTTP/1.1 (see stage 1).

## Stage 1: HTTPS serving in flux:http

- `ServeOptions.tls?: { cert, key, caCerts?, anyClientCert? }`: Bun's
  option name, with the member names of `ListenTlsOptions`. Without it,
  `serve` is as today. `http` implies `tls`, so a build with `serve`
  always has it.
- forge `http`: the accept loop hands each TCP stream to its connection
  task, which runs the handshake (bounded by the 10 s handshake limit of
  `listenTls`), so a stalled client holds up no other connection. Then an
  `h2` connection is served by hyper's http2 builder, an `http/1.1` one by
  today's http1 builder with upgrades. Both shut down gracefully on
  `close()`.
- The WebSocket upgrade: the server does not enable extended CONNECT (RFC
  8441). So a browser opens its WebSocket on a separate HTTP/1.1
  connection, and `upgrade()` keeps its meaning. Plain-HTTP `serve` stays
  HTTP/1.1 (no h2c).
- `forge/Cargo.toml`: hyper gains `http2` as a normal feature behind
  `http` (today it is a dev-dependency feature only).
- `server.url` is `https://...`. `server.peerKey(req)` beside
  `requestIP`: the client certificate's key in the `peerKey` form of a
  `Conn`, or `null`. `Remote` (or `RequestParts`) carries it.
- `p2p` with `tls`: `tls` covers the TCP listener only; a p2p connection
  is encrypted by iroh already.
- `packages/flux-types/modules/http.d.ts`: `ServeOptions.tls`,
  `Server.peerKey`, the `url` doc.
- Tests: forge, a round trip with the private CA over `http/1.1` and
  over `h2` (the client sees HTTP/2); mTLS with `peerKey`; a stalled
  handshake does not delay the next connection; a WebSocket upgrade on an
  HTTPS server. flux, `serve({ tls })` fetched through
  `createHttpClient({ caCerts })`, and `peerKey` with a client
  certificate.

## Stage 2: startTls as the server, and certificates

- `startTls(conn, { isServer: true, cert, key, alpn?, caCerts?,
  anyClientCert?, timeoutMs? })`. The options are `ListenTlsOptions`
  without `host`, with `timeoutMs` (default 10 s, as the client). The
  guarantees of the client upgrade hold: the old `Conn` is spent at once
  and no plain-text bytes are lost. `forge::net::start_tls` takes the
  role (client config and server name, or server config).
- `generateCertificate({ names, validDays?, isCa?, issuer? })` in
  `flux:net`, with the `tls` capability, returns `{ cert, key, pin }`:
  PEM certificate, PKCS#8 PEM key, and the key's pin in the `peerKey`
  form. `names` are the subject alternative names, DNS names and IP
  addresses alike. Without `issuer: { cert, key }` it is self-signed;
  `isCa` makes a CA that can be an `issuer`. The key is ECDSA P-256.
  `validDays` defaults to a named constant (proposed 10 years). It is
  synchronous: a P-256 key takes well under a millisecond. The runtime
  stores nothing; the app keeps its PEM (flux:fs). The verb is Web
  Crypto's (`generateKey`).
- `forge::tls::generate_certificate` on native types; the plugin
  marshals.
- `forge/Cargo.toml`: rcgen `=0.14.10`, `default-features = false`,
  features `crypto`, `pem`, `ring`, behind `tls`. Check Cargo.lock gains
  no aws-lc.
- `packages/flux-types/modules/net.d.ts`.
- Tests: forge, a self-signed certificate served by `listenTls` and
  trusted by its `pin`; a generated CA issues a leaf that a `caCerts`
  client trusts; an IP name verifies by IP. A plain greeting, then
  `startTls` on both sides of a `listen()` connection, with
  `anyClientCert` and `peerKey`. flux, the same upgrade pair, and
  `generateCertificate` into `serve({ tls })`.

## Stage 3: the WebSocket constructor options

- `new WebSocket(url, protocols?)`, the standard argument (a string or
  an array): the client sends `Sec-WebSocket-Protocol`, `ws.protocol`
  reports the server's pick (`""` for none), and a pick that was not
  offered fails the connection, as RFC 6455 requires.
- In place of `protocols`, an object `{ protocols?, client? }`, the shape
  undici's (Node) and Bun's `WebSocket` take. `client` is an `HttpClient`
  from `createHttpClient`: a `wss://` connection verifies with its roots,
  its pin and presents its client certificate. A closed client reports
  `error` then `close`, as a failed handshake does. On `ws://` the client
  has no effect.
- The `HttpClient` keeps the `forge::tls` client config it built reqwest
  from; `run_client` takes that config (ALPN set to `http/1.1` as today).
- A WebSocket takes no per-host slot, as today and as in browsers.
- `packages/flux-types/standards/websocket.d.ts`: the constructor,
  `protocol`, and the "not reachable here" note goes.
- Tests: forge, `run_client` with a pin config against a self-signed TLS
  WebSocket server; subprotocol offer, pick and mismatch. flux, now
  possible end to end with stage 1: `serve({ tls, websocket })`, and
  `new WebSocket("wss://127.0.0.1:<port>", { client })`; the server picks
  a subprotocol through the `upgrade` headers.

## Stage 4: one limiter per app

- `HostLimits` becomes `Send + Sync` (its state behind a mutex, held in an
  `Arc`). The root engine creates one; an isolate gets its parent's
  through `EngineConfig`, as it gets the logger and the resolver today.
  So every fetch of the app, from any isolate and any client, shares one
  cap and one cooldown per host.
- The flux test host builds every test engine new, so each test has its
  own limiter and a 429 cooldown does not leak into the next test.
- flux-types: the `fetch` doc says the limits hold across isolates.
- Tests: flux, a parent and an isolate fetch from one local HTTP/1.1
  server beyond the cap, and the server sees no more than the cap at
  once; a 429 seen by the isolate pauses the parent's fetches too.

## Stage 5: media sources and the per-host cap

Decision first, with the user; then build it. The question: does a
remote media stream (`forge::source`'s `HttpSource`) count against its
host's slots?

- Browsers count it. The known trap: on HTTP/1.1, six media requests to
  one host stall every other request to it.
- The options: (a) count it as browsers do, a slot per open request (a
  paused player with no request in flight holds none); (b) a separate
  media budget per host; (c) no slot, as today, documented.
- First measure what decides it: does an `HttpSource` keep a request open
  while its player is paused or full?
- Plumbing for (a) or (b): `HttpSource` is made without an engine, so the
  plugin that opens it passes the engine's limiter in. An HTTP/2 host is
  unaffected either way.

## Verification

- `free -g` before each cargo run; one test target at a time, with the
  usual job cap.
- The forge tests with `http` (which implies `tls`), then
  `make -C flux flux` and `bun run sol test` for the flux module tests.
- `cargo test -p flux --lib --features gui` after each stage that
  touches a plugin.

## Not in this plan

- DTLS for `Udp` (see Decisions).
- h2c, HTTP/2 without TLS: no browser speaks it.

## Findings
