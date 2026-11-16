---
title: Renew a TLS certificate without a restart
description: A server that `serve` (flux:http), `listenTls` (flux:net) or `serveRelay` (flux:p2p) runs with a certificate keeps it until the process stops, so an app whose CA-signed certificate renews (every 60 to 90 days with Let's Encrypt) has to restart the server; one reloadable certificate resolver in forge::tls would let all three swap the certificate in place.
created: 2026-11-16
---

# Renew a TLS certificate without a restart

## Symptom

`serve`, `listenTls` and `serveRelay` take `tls: { cert, key }` once, when
they start. `forge::tls::server_config` builds a rustls `ServerConfig` with
`with_single_cert`, and nothing can replace that certificate later. A
CA-signed certificate expires and renews (Let's Encrypt every 60 to 90
days, often with a new key), so today the app closes the server and starts
it again with the new PEMs.

For the relay that is cheap: a fixed `token` keeps the URL, the clients
reconnect on their backoff, and direct p2p paths are untouched. For
`serve` and `listenTls` a restart drops every open connection and the port
is briefly closed.

## Shape

- One reloadable resolver in `forge::tls`: a `ResolvesServerCert` that
  holds the current `CertifiedKey` behind a lock, used by `server_config`
  instead of `with_single_cert`. New handshakes get the new certificate;
  open connections keep theirs.
- One JS method per server with the same shape, e.g.
  `server.setCertificate({ cert, key })`, which parses and checks the pair
  (the key matches the leaf) and throws on a bad one before it swaps.
- For the relay: `Relay.url` carries a pin when the certificate is
  self-signed (or `tls.pinned: true`). A new key changes the pin, so a swap
  that changes the pin changes the URL; the method either rejects that or
  reports the new `url`. Decide which.

## Open

- Whether any app needs this before it runs behind a reverse proxy that
  renews for it (then the runtime serves plain HTTP on `127.0.0.1` and
  renews nothing).
- iroh-relay takes an owned `ServerConfig` in `CertConfig::Manual`; check
  that a custom resolver inside it keeps working after `Server::spawn`.

Related: okf/done/p2p-relay-on-a-public-network.md (the relay's `tls`, the
decision against a relay-only `setTls`).
