---
title: TLS sockets in flux:net
description: flux:net is plain TCP, so an app cannot speak a TLS protocol other than HTTPS (IMAP, SMTP STARTTLS, MQTTS, its own protocol) or serve one; add connectTls, startTls and listenTls that return the existing Conn and Listener.
created: 2026-11-04
completed: 2026-11-04
---

# TLS sockets in flux:net

## Problem

`flux:net` gives an app TCP streams (`connect`, `listen`) and nothing
above them. The only TLS in the runtime is inside `fetch` (reqwest on
rustls) and iroh's QUIC. So an app cannot:

- talk to a TLS server that is not HTTP: IMAPS, SMTPS, MQTTS, a game
  server, a database, its own protocol,
- upgrade a plain connection in protocol (SMTP and IMAP `STARTTLS`,
  XMPP, the PostgreSQL `SSLRequest`),
- serve TLS itself, for example a LAN server with a self-signed
  certificate that a paired client pins, and that identifies the paired
  client by its certificate in turn.

The app cannot do TLS in JS on top of `Conn` either: no TLS
implementation is exposed, and writing one in JS is not an option.

## Done looks like

- `connectTls(host, port, opts?)` opens a TCP connection, does the TLS
  handshake, and returns a `Conn`. By default it verifies the server
  against the webpki roots for `host`, as `fetch` does. `timeoutMs`
  covers the connect and the handshake together.
- `startTls(conn, opts)` upgrades an open plain `Conn` as the client and
  returns a new `Conn`. The old one is spent: its `readable` ends and its
  writes reject. `opts.hostname` names the server to verify and to send
  as SNI; the `Conn` only knows the peer's IP. It is required unless
  `pin` is given.
- `startTls` works whatever the app did with the old `readable`: read it
  with a reader and released the lock, broke out of a `for await`,
  cancelled it, or left a read pending. A pending read ends as
  end-of-stream.
- `listenTls(port, { cert, key, ... })` returns a `Listener` whose
  connections arrive with the handshake done. A client that connects and
  stalls does not hold up the next accept, and a failed or timed-out
  handshake never reaches the app.
- The client options (`connectTls`, `startTls`): `caCerts` (extra roots,
  PEM), `cert` and `key` (a client certificate for mTLS), `pin`, `alpn`
  (the protocols to offer), and `hostname` for `startTls`.
- The server options (`listenTls`): `cert` and `key` (the server
  identity, required), `alpn` (the protocols to accept, in preference
  order), and the client authentication: none by default; with
  `caCerts`, a client certificate that chains to them is required; with
  `anyClientCert: true`, a client certificate is required but any key is
  accepted, and the app checks `peerKey` itself (the pairing case).
- The `Conn` reports `alpnProtocol` (the negotiated protocol, or `null`)
  and `peerKey` (the SHA-256 of the peer's SubjectPublicKeyInfo, as
  lowercase hex, or `null` without a peer certificate). A plain TCP
  `Conn` reports `null` for both.
- `pin` is a SHA-256 SPKI hash in the same lowercase hex form `peerKey`
  reports, so a client can store what it saw at pairing and pin it.
- Half-close: `writable.close()` sends `close_notify` and then FIN.
  `close()` makes one non-blocking attempt to send `close_notify` and
  then drops the connection.
- A peer that closes without `close_notify` ends the `readable` as
  end-of-stream, not as an error.
- A failed handshake, a pin mismatch or a missing required client
  certificate rejects `connectTls`/`startTls` with a clear error.
- `Flux.capabilities` includes `"tls"` when the runtime has it.
- Types and docs in `packages/flux-types/modules/net.d.ts`, forge tests
  in `forge/src/tests/net.rs`, and `flux/tests/*.test.ts` cases:
  - a round trip `connectTls` to `listenTls`, with ALPN,
  - a private CA,
  - mTLS with `caCerts`, and with `anyClientCert` and `peerKey`,
  - a pin match and a pin mismatch,
  - `startTls` after a plain-text exchange read with a released reader,
    and after a `for await` that broke,
  - a stalled client that does not block the next accept,
  - half-close.

  The test certificates are PEM fixtures checked into the repo, with a
  long validity.

## Involves

**The JS shape.** Deno has the same three functions: `Deno.connectTls`,
`Deno.startTls` and `Deno.listenTls`, all returning the plain `Conn` and
`Listener`. Keep those names and Deno's `hostname` option for
`startTls`. Keep the positional forms of `flux:net`: `(host, port, opts)`
as `connect` has, `(port, opts)` as `listen` has. Returning the existing
classes means that `pipeTo`, half-close and `close()` work the same with
or without TLS. `alpnProtocol`, `peerKey`, `anyClientCert` and `pin` are
off Deno; Deno has no pinning and no way to read the peer certificate.

**forge.** A `forge::tls` module builds the rustls client and server
configs from native types (PEM bytes, pin bytes, ALPN strings).
`forge::net` wraps the `TcpStream` with tokio-rustls. `Conn` today holds
tokio's owned TCP halves (`OwnedReadHalf` / `OwnedWriteHalf`). It holds
an enum per half instead: the TCP half, or the half of a TLS stream split
with `tokio::io::split`. `ReadSlot<R>` and `read_half` are generic
already, so `forge/src/stream.rs` does not change.

**startTls and the read half.** `startTls` needs both TCP halves to
`reunite` the `TcpStream`. Today the read half does not survive the JS
side: a `break` out of `for await`, or a `cancel()`, returns the
iterator, which drops the `ByteStream`, which closes the `ReadSlot` and
drops the `OwnedReadHalf` (`flux/src/standards_plugins/body.rs`,
`byte_stream_iterable`). So a plain TCP `Conn` keeps its read half when
the view drops, until `Conn.close()`. On the wire this changes nothing:
dropping an `OwnedReadHalf` sends nothing and the socket stays open while
the write half lives. The p2p `Stream` keeps today's behavior, where
dropping the read half tells the peer to stop sending. A read pending at
the moment of `startTls` is no obstacle: the slot's lock is held only
inside a poll, so `startTls` takes the reader and ends the old view.

No plain-text bytes are lost to the upgrade. `ReadableStream.from` pulls
with a highWaterMark of 0, so the JS stream holds no chunk the app did
not ask for. A STARTTLS server waits for the ClientHello, so nothing
arrives after the last plain-text answer.

**The listener.** The handshakes run concurrently inside the `Listener`,
each bounded by a named `HANDSHAKE_TIMEOUT`: `accept` polls the TCP
accept and the handshakes in flight together, and yields the next
connection whose handshake finished. A handshake that fails or times out
drops its connection. The `for await` over a listener keeps one `accept`
pending, so the handshakes advance while the app handles a connection.
The alternative, Deno's lazy handshake on the first read or write, needs
a `handshake()` promise before `alpnProtocol` and `peerKey` can be read,
and moves the failures into the app's first read; the eager form keeps
both as plain properties.

**The pin.** It is the same verifier that
[fetch-tls-options](fetch-tls-options.md) needs, a custom rustls
`ServerCertVerifier`, built once in `forge::tls` so both items use it.
One rule for both:

- The leaf's SPKI hash must match the pin. The pin replaces both the
  chain check and the name check: the key is the identity, and a LAN
  server's self-signed certificate rarely names the address a client
  reaches it on (DHCP moves it).
- The handshake signatures are still verified, by delegating
  `verify_tls12_signature` and `verify_tls13_signature` to rustls with
  the ring provider's algorithms. A verifier that only hashes the
  certificate accepts anyone who replays the certificate without the key.
- The host name still goes out as SNI when it is a name.

The `anyClientCert` server side is the same idea mirrored: a custom
`ClientCertVerifier` that accepts any leaf, still verifies the
signatures, and leaves the identity check to the app through `peerKey`.

**Feature.** A new forge `tls` feature holds `forge::tls`, the three
functions and the `"tls"` capability, and `http` implies it: `fetch`
already depends on the same rustls, ring and webpki roots, and a TLS
socket is not HTTP. Without the feature, the three functions throw and
name the feature. tokio-rustls (=0.26.4) and webpki-roots (=1.0.11)
become direct, pinned dependencies at the versions already in the
lockfile. tokio-rustls must be declared with `default-features = false,
features = ["ring", "tls12", "logging"]`: its defaults include
`aws_lc_rs`, which would add a second crypto provider and its C build.
The configs are built with `builder_with_provider` and the ring
provider, so no process-wide default provider is needed.

**Half-close over TLS.** TLS 1.3 allows reading after sending
`close_notify`. A TLS 1.2 peer usually answers `close_notify` by closing
the whole connection, so "request done by half-close" works only on TLS
1.3. The `net.d.ts` doc for `writable.close()` says so.

## Out of scope, related

- `startTls` as the server (an app that serves SMTP or IMAP STARTTLS).
  An app's own protocol can use `listenTls` instead. Stage 2 of
  [tls-follow-ups](../plans/tls-follow-ups.md), with the `listenTls`
  server options.
- `wss://` in the WebSocket client (`forge/src/websocket.rs`). Its
  connector can use the same `forge::tls` client config once this lands.
  It landed as stage 4 of [tls-and-fetch-limits](tls-and-fetch-limits.md).
- DTLS for `Udp`. It stays out; the reason is in
  [tls-follow-ups](../plans/tls-follow-ups.md).
- Certificate generation (an app that wants a self-signed identity brings
  its PEM). Stage 2 of [tls-follow-ups](../plans/tls-follow-ups.md).

## Outcome

Stage 2 of [tls-and-fetch-limits](tls-and-fetch-limits.md).
Built as shaped, with these choices:

- A missing or rejected client certificate cannot reject `connectTls`
  on TLS 1.3: the client finishes its side of the handshake before the
  server checks the certificate, so the client sees the server's alert
  on its first read. `net.d.ts` says so; the forge test asserts it.
- `startTls` takes `timeoutMs` (default 10 s, as `connect`), which
  bounds the handshake.
- The `listenTls` handshakes run as tasks of a `JoinSet` inside the
  `Listener`, so they also advance between two `accept`s; closing the
  listener aborts them.
- `pin` and `caCerts` exclude each other on the client, `caCerts` and
  `anyClientCert` on the server: each pair names two contradicting
  trust rules, so the call throws.
- tokio-rustls and webpki-roots are pinned in `forge/Cargo.toml` only:
  forge is their one user, and the workspace table holds the shared
  dependencies.
- lattice passes `tls` through as a feature of its own, so a runtime
  without `http` can keep TLS sockets.
