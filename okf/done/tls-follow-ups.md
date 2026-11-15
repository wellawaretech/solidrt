---
title: Close the TLS gaps left by tls-and-fetch-limits
description: The gaps tls-and-fetch-limits left "until a real app needs it" - HTTPS (and h2) in serve, startTls as the server, certificate generation, subprotocols and a TLS client for WebSocket, the per-host limits shared across isolates, automatic TLS session resumption in flux:net - plus the open decision on media sources and the cap; one stage each.
created: 2026-11-05
---

# TLS follow-ups

## Problem

[tls-and-fetch-limits](tls-and-fetch-limits.md) left these out
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
- Media sources (`forge/src/source.rs`) take no per-host slot, and the
  cap itself is a private constant no app can see or change: see stage 5.
- A `flux:net` TLS connection cannot resume another one's session. Each
  `connectTls` / `startTls` builds a new rustls `ClientConfig`, and its
  session cache with it, so the second connection to a server always
  does a full handshake. Servers that require the second connection to
  resume the first refuse it: FTPS data channels (RFC 4217, `PROT P`)
  fail on vsftpd (`require_ssl_reuse`, on by default: `522 SSL connection
  failed; session reuse required`) and on FileZilla Server (`425 Unable
  to build data connection: TLS session of data connection not resumed`).
  Browsers, Node's `https.Agent` and reqwest resume on their own; see
  stage 6.

## Decisions taken (2026-11-05)

- One plan, six stages in the order below, each tested on its own.
- DTLS for `Udp` stays out: Node, Deno, Bun and the web expose none,
  rustls has none, and it would add a second TLS stack (OpenSSL) beside
  rustls.
- Certificates come from rcgen `=0.14.10` (the rustls project, on the ring
  provider as `forge::tls` is), behind the `tls` feature. It is in the
  tree already: iroh-relay and tokio-rustls-acme depend on it, so the pin
  adds no new crate.
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
  features `crypto`, `pem`, `ring` and `x509-parser` (for
  `Issuer::from_ca_cert_pem`), and time `=0.3.47` with `std` (the clock of
  `not_before`), behind `tls`. rcgen's defaults are the first three
  already and Cargo.lock holds no aws-lc today; the explicit list keeps a
  future default change from pulling it in.
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
  from. `run_client` takes an optional client config already; the
  WebSocket plugin passes the client's instead of building one (ALPN set
  to `http/1.1` as today).
- A WebSocket takes no per-host slot, as today and as in browsers.
- `packages/flux-types/standards/websocket.d.ts`: the constructor,
  `protocol`, and the "not reachable here" note goes.
- Tests: forge, `run_client` with a pin config against a self-signed TLS
  WebSocket server; subprotocol offer, pick and mismatch. flux, now
  possible end to end with stage 1: `serve({ tls, websocket })`, and
  `new WebSocket("wss://127.0.0.1:<port>", { client })`; the server picks
  a subprotocol through the `upgrade` headers.

## Stage 4: one limiter per app

- `HostLimits` becomes `Send + Sync` (the host map behind a mutex, each
  host's state in an `Arc`: its cooldown behind a mutex of its own, the
  HTTP/2 mark an atomic; no lock is held across an await). The root
  engine creates one (`EngineConfig::fetch_limits`); an isolate gets its
  parent's through `EngineConfig`, as it gets the logger and the resolver.
  So every fetch of the app, from any isolate and any client, shares one
  cap and one cooldown per host. The TLS client config cache of stage 6
  travels with it (`EngineConfig::tls_client_configs`).
- The flux test host builds every test engine new, so each test has its
  own limiter and a 429 cooldown does not leak into the next test.
- flux-types: the `fetch` doc says the limits hold across isolates.
- Tests: flux, a parent and an isolate fetch from one local HTTP/1.1
  server beyond the cap, and the server sees no more than the cap at
  once; a 429 seen by the isolate pauses the parent's fetches too.

## Stage 5: media sources and the per-host cap

Decided 2026-11-14: one explicit rule at the lowest layer, and the cap
itself under the app's control.

- The rule: a slot is an open request to a host, whatever opened it. A
  remote media stream (`forge::source`'s `HttpSource`) counts against its
  host's slots as every fetch does, and as browsers count it; a player
  with no request in flight holds none, and a paused one keeps its request
  open (the measurement under Findings), so it keeps its slot. An HTTP/2 host is
  unaffected either way. Rejected: a separate media budget per host, a
  second policy baked into the runtime with no knob, and "no slot,
  documented", which leaves the browser trap (six streams hold every slot
  of an HTTP/1.1 host) as the only behaviour an app can have.
- Full control: the per-host cap stops being a private constant. `flux:http`
  exports a small explicit surface over the app's one limiter (stage 4):
  read the cap, set it (per host, or the default for every host; a host can
  be lifted to unlimited), and the two consumers that open long-lived
  requests take an opt-out: a media source opened outside the cap (an
  option on the player/source open call), and `fetch(url, { slot: false })`
  or an equivalent named option for a request that must not queue behind
  the cap. The default stays 6 for HTTP/1.1 hosts, as browsers cap, and the
  standard names and shapes stay; the semantics are documented plainly in
  the `fetch` doc of flux-types.
- First measure what decides how much the opt-out matters: does an
  `HttpSource` keep a request open while its player is paused or its buffer
  is full (one long request under TCP backpressure), or does it fetch
  ranges on demand (a paused player holds no request)? Record the answer
  under Findings before building.
- Plumbing: `HttpSource` is made without an engine, so the plugin that
  opens it passes the engine's limiter (the app's, after stage 4) in, and
  the opt-out passes `None`. The cap surface reads and writes the same
  `HostLimits`, which gains a per-host override beside its default.
- Tests: forge, a source against a local HTTP/1.1 server holds a slot
  while its request is open, a full ring included (the measurement: a
  paused player keeps its request), frees it when the reader closes, and
  holds one, not two, across a reopen; a source without limits takes none.
  The settable cap: a raise frees a waiter at once, a lowering applies as
  slots free, unlimited takes no slot, a per-host override beside the
  default, `slot: false` skips the cap but not a cooldown. flux, the cap
  read back as the default and per host, a lowered default caps a burst at
  the server, an unlimited host does not, `slot: false` goes past a cap of
  1, and the validation errors. `video.open(.., { slot })` has no test of
  its own: the flux module test host has no GUI, and no app test opens a
  video.

## Stage 6: TLS session resumption in flux:net

- Resumption is automatic, no API: a `connectTls` or `startTls` whose
  client options (`caCerts`, `cert` and `key`, `pin`, `alpn`) equal an
  earlier connection's offers to resume that connection's session with
  the same server name, as browsers, Node's `https.Agent` and reqwest
  do. So FTPS works as is: the control and data connections carry the
  same options, so the data connection resumes without the app asking.
  Rejected: one store per engine keyed by server name alone, which
  crosses trust policies: a resumed handshake verifies no certificate, so
  a pin-only connection could ride on a session verified against the web
  roots only, and a connection without a client certificate could inherit
  another's identity.
- Full control (decided 2026-11-14, after the first cut landed with no
  knob): `connectTls` and `startTls` take `resume?: boolean | Conn`.
  `true` (the default) is the automatic behaviour above. `false` opts the
  connection out: a full handshake every time, for a server that
  misbehaves on resumption or when the app wants a fresh certificate
  check (forge: the call gets a `ClientConfig` with `Resumption::disabled()`
  built from the same options, not the cached one, so the cache is not
  polluted). A `Conn` asks to resume that connection's session: it must be
  a TLS client connection whose options identity equals this call's, else
  the call throws before connecting (`resume: the connection's TLS options
  differ`), which turns the silent FTPS failure (a full handshake the
  server refuses) into a visible one. A `Conn` gives no stronger guarantee
  than equal options do (rustls keys sessions by server name under the
  shared config), so forge records each TLS client `Conn`'s options
  identity for the check, and nothing more. The two cache bounds stay
  private: a memory ceiling, not a behaviour. Lands after stage 2, which
  touches the same files.
- forge: a `ClientConfigs` cache in `forge::tls`, keyed by the options'
  identity (the PEM bytes, the pin and the ALPN list hashed,
  length-prefixed, `Send + Sync`), hands out one `Arc<ClientConfig>` per
  distinct key, bounded by `CLIENT_CONFIG_KEYS` with least-recently-used
  eviction (`config_for(&ClientOptions)`). Each config keeps rustls's
  default in-memory session store, so session equality is options plus
  server name, rustls's own key. The config is cached, not a session
  store per identity: rustls resumes a session only under the very same
  verifier and client-credential objects it was made under (an `Arc`
  identity check), so a store shared between freshly built configs never
  resumes.
- flux: one `ClientConfigs` per app, made by the root engine's builder
  and carried in `EngineConfig` (`tls_client_configs`, beside the fetch
  limits since stage 4), so isolates share the parent's; `flux:net`
  stores it in userdata (`TlsClientConfigs`) and asks it for the config of
  the parsed options of `connectTls` and `startTls`. `createHttpClient`
  builds its own, as before.
- `conn.resumed: boolean` on a TLS `Conn` (rustls `handshake_kind()`), so
  an app and the tests can see that it resumed.
- The TLS 1.3 trap: the server sends its session ticket after the
  handshake, and the client stores it only when it reads from that
  connection. The doc says so: read from the first connection (FTP reads
  the control reply anyway) before opening the second. With no option to
  force resumption, this note is the one thing an app can act on.
- `packages/flux-types/modules/net.d.ts`: the resumption contract on
  `connectTls` and `startTls`, `resumed`, the trap.
- Tests: forge, a `listen_tls` server and two connections with equal
  options report a resumed handshake over TLS 1.2 and 1.3; a second
  connection with a different ALPN list reports a full one, and a
  different pin or an added client certificate gets its own config.
  Neither `client_config` nor `server_config` has a protocol-version knob
  (both take the safe defaults), so `server_config` calls a crate-private
  `server_config_with_versions` the test calls with TLS 1.2 only; the app
  surface does not change. flux, two `connectTls` to one server, the
  second `resumed`, and a `connect()` plus `startTls` after a
  `connectTls` with the same options. Check once by hand against a vsftpd
  with `require_ssl_reuse` and `PROT P`.

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

- 2026-11-14, stage 5 landed. `HostLimits` holds the default and each
  host's state under one map lock; a host's cap is a pool (a semaphore and
  a ledger of its cap and the permits it owes), `None` while unlimited.
  `set_limit(host, limit)` sets a host's override or the default (which
  every host without an override follows, seen or not yet seen);
  `limit(host)` reads it back. A raise repays what is owed, then adds
  permits. A lowering forgets the free permits at once and owes the rest;
  the debt is paid on the acquire side, not the release side: an acquire
  that gets a permit from a pool that owes forgets it and waits on. That
  keeps `acquire`'s `OwnedSemaphorePermit` (no wrapper with a drop hook),
  and the effect is the same: no new request starts until the open ones
  are within the new cap. Unlimited closes the pool, so its waiters go at
  once with no slot; a finite cap afterwards starts a fresh pool, which
  does not count the requests opened meanwhile. A cap of 0 is taken as 1,
  and one above the semaphore's maximum as that maximum. Four things the
  plan did not say. First, `host_key` takes a URL only, so the cap surface
  resolves its `host` with a new `fetch::parse_host_key`: a URL, or a
  `host:port` key (the port required, the host lowercased as a URL's is).
  Second, `do_fetch` and `do_fetch_cached` take `slot: bool` after the
  limits; without a slot a request waits out the cooldown through
  `HostLimits::cooled`. `do_fetch_cached` now has nine parameters and an
  `allow(clippy::too_many_arguments)`. Third, `Source::open` takes
  `Option<source::Limits>`, which is `Arc<HostLimits>` with `http` and an
  uninhabited enum without it, so the video plugin passes `None` there
  with no cfg on the call. `HttpSource::open_at` takes the slot before
  the send, records the HTTP version after it, and wraps the body with
  `fetch::with_permit`. Fourth, the option readers share a new
  `marshal::bool_opt` (`fetch: slot must be a boolean`, `openVideo: slot
  must be a boolean`, both a `TypeError`, as are the `getHostLimit` and
  `setHostLimit` errors). `flux:http`'s two exports live in
  `forge_plugins/host_limit.rs`.

- 2026-11-14, stage 4 landed as planned. `EngineConfig` carries
  `fetch_limits: Arc<HostLimits>` (behind `http`) and
  `tls_client_configs: Arc<ClientConfigs>` (behind `tls`); `build` makes
  them when the builder has none (`FETCHES_PER_HOST` moved to
  `engine.rs`), `from_config` copies them, and there is no public setter.
  `init_http` and `flux:net` read them off the stored config. The flux
  test needed one change outside the plan: a flux module test is a stdin
  bundle whose cwd is the sandbox, emptied ahead of every test engine, so
  the `flux` binary's disk resolver (`./isolates/<id>.js`) found no
  checked-in module there. Its `--test` mode now resolves isolates through
  the assets mount (`resolve_isolate_from_assets`, as the runtime does);
  `sol test` mounts the package directory (`--assets flux`), so a module
  test's isolates are `flux/isolates/<id>.js`. A plain `flux <file.js>`
  run reads `<entry dir>/isolates/` as before. `fetch-limits.test.ts`
  sees a server peak of 6 in flight from 20 fetches of the parent and an
  isolate (12 when the isolate does not inherit the limits), and a parent
  fetch after the isolate's 429 with `Retry-After: 1` reaches the server
  no sooner than 1 s after it.

- 2026-11-14, stage 3 landed as planned. `WsTarget` carries the offered
  `protocols` (empty from `parse_ws_url`); `run_client` sends them as one
  `Sec-WebSocket-Protocol` header and reads the 101's answer, which
  fastwebsockets' `handshake::client` returns with the socket. No answer
  agrees on `""`; one offered value agrees on it; anything else (a value
  not offered, a value when none was, several values) drops the socket
  and reports `connect failed: the server picked a subprotocol that was
  not offered: <values>`, then 1006. The pick sits on `ClientSocket`
  (`protocol()`), set before `open`. Placement: the token check is
  `forge::websocket::check_protocols` (a non-empty RFC 7230 token, no
  duplicate), so the plugin only reads the argument and throws a
  `SyntaxError` with its message; a `client` that is no `HttpClient`
  throws a `TypeError`, as `fetch` does. `HttpClient::tls()` is `None`
  once closed, like `client()`; the closed-client path reports through
  `ClientSocket::fail` (error, then 1006), which the failed connect now
  shares, since the socket state is forge's. A `null` second argument
  reads as absent, not as the protocol `"null"` WebIDL would make of it.
  The global `websocket.d.ts` names the client type as
  `import("flux:http").HttpClient`, as `fetch.d.ts` does, since a
  top-level import would make the global file a module. The review tightened three things: the second argument accepts a plain
  object only (a function, a typed array or a promise throws, as a
  number does), a client closed at construction logs the failure like
  every other failed connect, and a server that answers more than one
  subprotocol gets its own message.
- 2026-11-14, the stage 6 `resume` option landed as the "Full control"
  bullet says. forge's `connect_tls` and `TlsRole::Client` take the
  options identity from their caller (`Option<tls::ClientIdentity>`, now
  public with `tls::client_identity`), and the `Conn` hands it back as
  `client_identity()`; flux passes it on every client connection, also
  under `resume: false`, so a later `resume: thatConn` fails only when the
  options differ. `client_config_without_resumption` builds the config
  afresh with `Resumption::disabled()`, which neither offers nor stores a
  session, so the cached config's store stays as it was: the flux test's
  third connection with the default still resumes. With `isServer: true`,
  `resume` joins `hostname` and `pin` among the client-only options
  (`startTls: resume is a client option, not one with isServer`).
- 2026-11-14, stage 6 landed. Two deviations from the plan text. First,
  the TLS 1.2 knob is a crate-private `server_config_with_versions`, not a
  `pub(crate)` field on `ServerOptions`: flux builds `ServerOptions` by
  struct literal, which a crate-private field would break. Second, the
  cache holds whole client configs, not session stores: rustls 0.23
  (`compatible_config` in `msgs/persist.rs`) offers a stored session only
  when the config's verifier and client-cert resolver are the same `Arc`s
  that made it, so a shared store under new configs never resumed (the
  first forge tests showed it; only a rustls trace log says why). TLS 1.2
  needed nothing beyond the versions list: the default server config
  resumes by session id from its in-memory cache, and the client stores
  the session at the end of the handshake. TLS 1.3 resumes from the
  server's stateful tickets, which the client takes in on its first read.
  The by-hand check against a vsftpd with `require_ssl_reuse` is not done
  yet.
- 2026-11-14, stage 1 landed. hyper's HTTP/2 server spawns a task per
  stream through its `Executor`, which has no `Send` bound, so
  `forge::http::LocalExecutor` hands them to the host's spawner (flux:
  `ctx.spawn`, the forge tests: `spawn_local`). It carries a lifetime: a
  flux request future borrows the engine (`'js`), and `ctx.spawn` takes
  `'js` futures, so a `'static` executor does not fit the plugin.
  hyper-util's `TokioExecutor` needs `Send` and does not fit either. Two
  things the plan did not say. First, hyper hands an HTTP/2 request's
  target to the service as an absolute URI (from `:scheme`, `:authority`
  and `:path`); `RequestParts.url` takes its path and query over HTTP/2,
  so `req.url` and the routes see `/x?y` on both protocols. Second, an
  HTTP/2 request has no `Host` header (the authority is a pseudo-header),
  so `serve` synthesizes one from the authority when the client sent none,
  and a handler reads `req.headers.get("host")` on both protocols (the
  review caught the first cut dropping it silently). The TLS handshake
  also watches the stop signal: a `close()` during a handshake drops that
  connection instead of serving it once. Under `anyClientCert` rustls refuses the
  handshake of a client without a certificate (the verifier makes client
  auth mandatory); over TLS 1.3 the client hears of it on its first read.
  A graceful shutdown of an HTTP/2 connection whose client already went
  away logs a broken pipe (the GOAWAY write); HTTP/1.1 writes nothing
  there.

- 2026-11-14, the stage 5 measurement, by reading `forge/src/source.rs`
  rather than by running it: an `HttpSource` is one long `GET` with
  `Range: bytes=N-`, pumped into a bounded ring (`STREAM_RING_BYTES`). When
  the ring is full, `push` waits on the ring's `space` signal with the
  response body still open, so the request is held under TCP backpressure
  for as long as the consumer does not read: a paused player with a full
  ring holds one open request to its host indefinitely. A seek outside the
  ring restarts the producer under a new request. So the browser trap
  applies as is: six paused players on one HTTP/1.1 host would hold all
  six slots once media counts, which is what makes the per-player opt-out
  and the settable cap of stage 5 necessary rather than nice to have.
- 2026-11-14, stage 2 landed. Cargo.lock gained no package: rcgen, time
  and x509-parser were in the tree already (iroh-relay), and the lock's
  only change is `rcgen` and `time` in forge's dependency list; it still
  holds no aws-lc. rcgen needs `x509-parser` beside `pem` for
  `Issuer::from_ca_cert_pem`, and time needs `std` for `now_utc`. Three
  choices the plan did not make. A `validDays` of 0, or one past the last
  date `time` holds, is an error (the plugin also refuses a fraction or a
  non-number). The first name is the subject's common name as well, in
  place of rcgen's placeholder (`rcgen self signed cert`), so a CA and its
  leaves do not share one subject. A server-side `startTls` gets a config
  of its own per call, so its sessions are its own and no client resumes
  into it. The flux STARTTLS test server still relays into a `listenTls`
  listener: the resumption test needs the upgrade and the direct TLS
  connection to share one server session cache, which two configs do not,
  so the server-side upgrade has tests of its own instead. The review added
  three checks to `generate_certificate`: a name that is neither an IP
  address nor a DNS name (an optional `*.` label allowed) is refused, since
  rcgen accepts any ASCII as a dNSName and the failure would only show at
  the client as `NotValidForName`; an issuer key that does not belong to
  the issuer certificate is refused (rcgen signs with whatever key it gets
  and the leaf then fails `BadSignature` everywhere); and the certificate
  is valid from `CERT_BACKDATE` (an hour) before issue, so a device whose
  clock runs behind accepts it at once.
