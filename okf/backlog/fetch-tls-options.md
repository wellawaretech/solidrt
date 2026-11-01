---
title: TLS options for fetch
description: fetch verifies servers against the webpki roots only and has no client certificate, so an app cannot reach a private-CA or self-signed server, pin a server certificate, or do mutual TLS; add a client certificate, extra roots and a pin, through a client object the app creates.
created: 2026-10-30
---

# TLS options for fetch

## Problem

`fetch` runs on reqwest with rustls, and trusts exactly the webpki root
set. Each engine builds one client with only a user agent set
(`forge::fetch::Client::new`, built in
`flux/src/standards_plugins/http.rs`), and `RequestInit` in
`packages/flux-types/standards/fetch.d.ts` has no TLS field. So an app
cannot:

- present a client certificate (mutual TLS),
- trust a server whose certificate is self-signed or from a private CA,
- pin a server certificate (trust on first use, or a fingerprint
  exchanged at pairing) instead of trusting any CA.

These are the normal shapes for an app that talks to its own server or to
a self-hosted one. `flux:net` is plain TCP, so the app cannot do TLS
itself either; nothing in the runtime is a workaround.

## Done looks like

- An app can give `fetch` a client certificate and key (PEM), extra
  trusted roots (PEM), and a pinned server identity, and the request
  fails with a TypeError when the pin does not match.
- Without any of these, `fetch` behaves exactly as today.
- Types, the flux-types docs and a test for each of the three: mTLS
  against a `flux:http` server or a local rustls server in the test,
  a private-CA server, a pin match and a pin mismatch.

## Involves

**The JS shape.** Two existing precedents, both off-standard (the web has
no TLS options at all):

- Deno: `Deno.createHttpClient({ caCerts, cert, key })` returns a client,
  and `fetch(url, { client })` uses it.
- Bun: a per-request `tls: { ca, cert, key, checkServerIdentity }` in
  `RequestInit`.

The client object fits the runtime better. A reqwest client is built
around one TLS configuration, so a client object maps one to one onto a
`forge::fetch::Client`, keeps connection pooling per configuration, and
validates the certificates once at construction instead of on every
request. A per-request option would need a client cache keyed by the TLS
configuration to get the same result. Decide the name and where the
constructor lives (a global next to `fetch`, or a `flux:http` export)
before building.

**The pin.** Pin the SHA-256 of the server's SubjectPublicKeyInfo (the
HPKP / OkHttp `CertificatePinner` form), not of the whole certificate, so a
server can renew a certificate on the same key. A pin replaces CA
verification for that client: the chain does not have to reach a trusted
root, but the leaf must match the pin and be valid for the host name. That
needs a custom rustls `ServerCertVerifier`, handed to reqwest with
`use_preconfigured_tls`. Client certificates (`Identity::from_pem`) and
extra roots (`add_root_certificate`) are plain reqwest builder calls.

**Layering.** forge takes the TLS configuration as native types (PEM bytes,
pin bytes) and builds the client; the standards plugin only marshals the
options and holds the client handle. All of it stays behind the `http`
feature.

## Out of scope, related

- `wss://` is rejected by the WebSocket client (`forge/src/websocket.rs`,
  "no TLS"). When it gets TLS, it should accept the same client so one
  configuration covers both.
- `forge::source` builds its own reqwest clients per user agent; remote
  media sources keep the default TLS.
