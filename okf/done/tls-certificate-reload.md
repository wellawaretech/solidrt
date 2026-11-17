---
title: Renew a TLS certificate without a restart
description: Shipped - serve (flux:http), listenTls (flux:net) and serveRelay (flux:p2p) take setCertificate({ cert, key }), which checks the pair and swaps it for new handshakes while open connections keep theirs; one reloadable resolver (forge::tls::ServerCert) backs all three, and a relay's url never changes, so a pinned relay renews only on the same key.
created: 2026-11-16
completed: 2026-11-17
---

# Renew a TLS certificate without a restart

## Symptom

`serve`, `listenTls` and `serveRelay` took `tls: { cert, key }` once, when
they started. `forge::tls::server_config` built a rustls `ServerConfig` with
`with_single_cert`, and nothing could replace that certificate later. A
CA-signed certificate expires and renews (Let's Encrypt every 60 to 90
days, often with a new key), so the app closed the server and started it
again with the new PEMs.

For the relay that is cheap: a fixed `token` keeps the URL, the clients
reconnect on their backoff, and direct p2p paths are untouched. For
`serve` and `listenTls` a restart drops every open connection and the port
is briefly closed.

## What shipped

- `forge::tls::ServerCert`: the certificate and key a server presents, a
  `ResolvesServerCert` over one shared slot. `server_config` builds the
  config with it as its resolver (in place of `with_single_cert`) and
  returns both as `ServerTls { config, cert }`: the config goes to the
  acceptors, the certificate to the server's owner. `set(cert, key)` parses
  and checks the new pair, then swaps; a bad pair leaves the current one.
- `setCertificate({ cert, key })`, synchronous, on `Server` (flux:http),
  `Listener` (flux:net) and `Relay` (flux:p2p). New handshakes get the new
  pair; open connections keep theirs. A `generateCertificate` result passes
  as it is. `forge::net::Listener::set_certificate` and
  `forge::p2p::Relay::set_certificate` hold the logic; the plugins forward.
- A key that is not the leaf's now reads `key: does not belong to cert`
  (at the start and at a swap), the wording `generateCertificate` uses for
  its issuer, in place of rustls's `keys may not be consistent:
  KeyMismatch`.

## Decisions

- **A relay's `url` never changes, so a pinned relay rejects a new key.**
  The alternative was to accept it and report a new `url`. A new pin
  breaks every client: tickets, QR codes and configs carry the old one,
  and an endpoint checks a known relay by the pin it has and cannot take a
  new one (okf/done/p2p-relay-on-a-public-network.md). Reporting a new URL
  is a restart in disguise, with each client failing later at its next
  reconnect. The full-control path stays: close the relay and serve a new
  one with the new key and its new URL. Whether a relay is pinned is fixed
  at its start and is not derived again from the new leaf. An unpinned
  (CA-signed) relay takes any key, because clients check it by its chain:
  that is the Let's Encrypt renewal.
- **Only the identity changes.** The client policy (`caCerts`,
  `anyClientCert`), the ALPN list and a relay's `pinned` stay as the server
  started. Passing one of them to `setCertificate` throws, so a call meant
  to change it does not pass silently. Node's `setSecureContext` replaces
  the whole context; this replaces the certificate and key only, hence the
  narrower name.
- **One method name and shape on all three servers**, synchronous like
  `setAllow`: parsing a PEM pair and swapping a lock is sub-millisecond.
- **No new observable.** The swap is the app's own call; the runtime does
  nothing on its own here.
- **The method exists in every build.** A `#[cfg]` on a method inside
  `#[rquickjs::methods]` does not hide it from the macro, and flux's
  `p2p-relay` feature does not turn on flux's `tls`. So
  `Listener::set_certificate` is built without TLS too, where every
  listener is plain and it errors (as the `Conn` TLS getters answer
  "plain" there).
- **Resumption across a swap is allowed.** A session made before the swap
  resumes after it with no certificate shown: that session was verified
  under the old, valid certificate, and its secrets do not depend on the
  certificate's key. Tests that check a refusal after a swap use a full
  handshake (`resume: false`, or a fresh client).

## Findings

- iroh-relay 1.4.1 `CertConfig::Manual` only wraps the owned
  `ServerConfig` in an `Arc` and a `TlsAcceptor` (`server.rs`, the
  `CertConfig::Manual` arm of `Server::spawn`), and rustls asks the
  config's resolver at every handshake, so a resolver inside it keeps
  working after `spawn`. `an_unpinned_relay_renews_its_certificate`
  (forge/src/tests/p2p_relay.rs) proves it against the running server.
- rustls 0.23 `CertifiedKey::keys_match` compares the key's
  SubjectPublicKeyInfo with the leaf's; every key type of the ring
  provider (RSA, ECDSA, Ed25519) exposes its public half, so a mismatch is
  always found.

## Open

- Whether any app needs this before it runs behind a reverse proxy that
  renews for it (then the runtime serves plain HTTP on `127.0.0.1` and
  renews nothing). It costs little either way.

Related: okf/done/p2p-relay-on-a-public-network.md (the relay's `tls`, the
decision against a relay-only `setTls`).
