---
title: TLS options for fetch
description: fetch verifies servers against the webpki roots only and has no client certificate, so an app cannot reach a private-CA or self-signed server, pin a server certificate, or do mutual TLS; add a client certificate, extra roots and a pin, through a client object the app creates.
created: 2026-10-30
completed: 2026-11-04
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
server can renew a certificate on the same key. The pin is a lowercase hex
string, the form `flux:net` reports as `peerKey`. It is the verifier of
[net-tls](net-tls.md), with the same rule:

- The leaf's SPKI hash must match the pin. The pin replaces both the
  chain check and the name check: the key is the identity, and a
  self-hosted server's certificate often does not name the address the
  app reaches it on.
- The handshake signatures are still verified, by delegating
  `verify_tls12_signature` and `verify_tls13_signature` to rustls with
  the ring provider's algorithms.
- The host name still goes out as SNI when it is a name.

**Layering.** `forge::tls` (from [net-tls](net-tls.md)) builds the rustls
`ClientConfig` from native types (PEM bytes, pin bytes): the roots, the
client certificate and the pin verifier. `forge::fetch` hands that config
to reqwest with `use_preconfigured_tls`, so one TLS configuration code
path serves `fetch`, `flux:net` and later `wss://`. reqwest keeps its own
connector and pool; only the TLS config is shared. reqwest uses a
preconfigured config as it is and sets no ALPN on it (reqwest 0.12,
`async_impl/client.rs`), so `forge::fetch` sets `alpn` itself:
`h2` and `http/1.1`, since [fetch-host-limits](fetch-host-limits.md)
turned reqwest's `http2` on. The
standards plugin only marshals the options and holds the client handle.
All of it stays behind the `tls` feature, which `http` implies. Whichever
of the two items lands first builds `forge::tls`.

## Out of scope, related

- `wss://` is rejected by the WebSocket client (`forge/src/websocket.rs`,
  "no TLS"). When it gets TLS, it should accept the same client so one
  configuration covers both. `wss://` landed as stage 4 of
  [tls-and-fetch-limits](tls-and-fetch-limits.md), with the default roots
  only.
- `forge::source` builds its own reqwest clients per user agent; remote
  media sources keep the default TLS.

## Outcome

Stage 3 of [tls-and-fetch-limits](tls-and-fetch-limits.md).
`createHttpClient({ caCerts, cert, key, pin })` is a `flux:http` export,
used as `fetch(url, { client })`; `close()` releases the client, and a
fetch through a closed one rejects. Every client shares the engine's
per-host limits. Built as shaped, with these choices:

- A pin mismatch rejects with a plain `Error`, as every other network
  failure of `fetch` does in flux, not a `TypeError`.
- A failed request's message now carries its cause: reqwest's own
  ("error sending request for url (..)") is followed by each source
  once, so a TLS failure says which check failed.
- The end-to-end "an HTTP/2 host takes no slot" case of
  [fetch-host-limits](fetch-host-limits.md) is in `forge/src/tests/fetch.rs`,
  against a hyper HTTP/2 server over tokio-rustls (hyper's `http2` as a
  dev-dependency feature only).
