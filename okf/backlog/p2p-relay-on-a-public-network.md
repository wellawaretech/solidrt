---
title: Let serveRelay run on a public network
description: Alice shows a QR code, Bob scans it, they connect through a relay, on a public network too; today the relay is plain HTTP with a random token per run and no bind address, allowlist or URL override, the endpoint cannot trust a self-signed relay, `connect` ignores the relay a ticket names, and a relay that never comes online fails silently. The plan is decided (pin in the relay URL, tickets adopted per connection) and ready to implement.
created: 2026-11-16
---

# Let serveRelay run on a public network

## Goal

Alice shows a QR code (her ticket), Bob scans it, and they connect through a
relay. Two cases, both supported:

1. **A shared relay** both apps know beforehand: the common case. Works today
   on plain HTTP.
2. **Alice's own relay**, which Bob learns from the QR code. They still meet
   on one relay (relays do not forward to each other); Bob just has to use
   the one the ticket names.

And the relay can sit on the internet: TLS, a stable URL, per-client access.

## Symptom

`serveRelay` (forge/src/p2p/relay.rs) was built for one shared network. The
gaps:

1. **No TLS.** `RelayOptions` is `{ port }`. On a public network plain HTTP
   exposes the token (the relay's only access control) and the metadata:
   which endpoint ids are connected, who relays to whom, when.
2. **No bind address.** The relay listens on `Ipv4Addr::UNSPECIFIED`, so it
   cannot sit on `127.0.0.1` behind a TLS reverse proxy.
3. **The token cannot be set.** A fresh random one per run changes the relay
   URL on every restart and breaks every ticket and config that holds it.
4. **No per-endpoint access control.** One shared token: a leaked token opens
   the relay and no single client can be revoked.
5. **`Relay.url` is always `http://<default-route IPv4>:<port>/`.** Behind a
   NAT, a proxy or a DNS name the URL to hand out is built by hand.
6. **The endpoint trusts only the built-in web roots** for the relay's
   certificate: no `caCerts`, no `pin`, so a self-signed relay is unusable.
   (`https://` itself parses, and iroh dials `wss`.)
7. **The docs describe `http://` only** (`relayUrl`, `Relay.url` in
   packages/flux-types/modules/p2p.d.ts).
8. **`connect(ticket)` ignores the ticket's relay** (case 2 above). The
   endpoint only uses the relay it was created with. With no `relayUrl` iroh
   has no relay transport at all; with another `relayUrl` iroh dials the
   ticket's relay on demand but sends no token (it looks tokens up per URL in
   its relay map) and the relay refuses. Today the app would have to cut the
   relay URL out of the ticket string by hand and create its endpoint with it.
9. **A relay that never comes online fails silently.** `ticket()` waits
   `TICKET_WAIT` on `online()`, then encodes `id||`; a peer's `connect` fails
   later with "No addressing information available". Every option above
   produces this when misconfigured (wrong pin, denied id, bad token). This
   was a section of okf/backlog/p2p-relay-across-nats.md; it is folded in
   here (step 1) and leaves that note.

## The pin, briefly

A pin is the SHA-256 of the relay's public key (the certificate's
SubjectPublicKeyInfo), 64 hex: the `pin` of `generateCertificate`, the form of
`TlsClientOptions.pin` and `Conn.peerKey`. The relay makes it from its own key
pair; the clients (Alice's and Bob's endpoints) check it:

```
setup, on the relay        generateCertificate() -> key (secret), cert (public key), pin
                           serveRelay({ tls: { cert, key } })
                           Relay.url = https://<token>@host:port/?pin=<pin>  -> QR, ticket, config
connect, every client      relay sends cert + a signature made with key
                           client: SHA-256(public key in cert) == pin?  signature valid?  else refuse
```

It replaces the chain check and the name check, so it is what a self-signed
relay needs; a relay with a web-root certificate needs none. It stays the
same as long as the key pair does: the app saves the relay's `cert` and `key`
once and reuses them (as it saves an endpoint's `secretKey`); a new key
changes every URL and ticket. `generateCertificate` cannot re-sign an
existing key, but its default validity is 10 years.

The pin protects only the hop to the relay. Alice and Bob authenticate each
other end to end by endpoint id, as today; a fake relay still reads nothing,
but would see the token and the metadata.

## What iroh 1.4.1 offers

Paths are under `~/.cargo/registry/src/*/`.

- One rustls (0.23.46) across the tree, so `forge::tls` configs hand straight
  to iroh.
- Relay server (iroh-relay-1.4.1/src/server.rs): `RelayConfig::new(addr)` takes
  the bind address. `RelayConfig::tls = Some(TlsConfig::new(https_addr,
  CertConfig::Manual { server_config }))` adds an HTTPS listener and keeps the
  plain one (`http_bind_addr`), whose only job then is the captive-portal
  probe. Clients send that probe to `http://<host>/generate_204`, always port
  80 (iroh-1.4.1/src/net_report/reportgen.rs:623), never our port.
  `CertConfig::LetsEncrypt` is there too; the `server` feature we build
  already pulls `tokio-rustls-acme`.
- Access (server.rs ~190-300): `AccessControl::on_connect(&ClientRequest)` sees
  the token (`auth_token()`) and the endpoint id the relay handshake proved
  (`endpoint_id()`); `on_disconnect(endpoint_id, connection_id)` pairs with
  it. `server.relay_service().clients().disconnect(id, None)`
  (server/clients.rs:181) drops a live client.
- Endpoint trust (iroh-relay-1.4.1/src/tls.rs):
  `Endpoint::builder().ca_tls_config(CaTlsConfig::custom_server_cert_verifier(..))`
  takes our own verifier; no `test-utils` feature needed. It is endpoint-wide
  (every relay), and a rustls verifier sees the server name but not the port.
- Relay list at runtime (iroh-1.4.1/src/endpoint.rs:998):
  `endpoint.insert_relay(url, Arc<RelayConfig>)` / `remove_relay`. The token
  of a relay is read from that map when iroh opens a connection to it
  (src/socket/transports/relay/actor.rs ~1344). `RelayMode::Disabled` means no
  relay transport at all (endpoint.rs:158).
- Status (endpoint.rs:1406, 1922): `home_relay_status()` is a watcher of
  `RelayStatus { url, is_connected(), last_error(), auth_denied_reason() }`;
  `last_error` covers a certificate failure, `auth_denied_reason` a token or
  allowlist denial.

## Decisions

- **Copy the TLS shapes where they fit.** `host` as in `serve`/`listen`
  (default `"0.0.0.0"`). `tls: { cert, key }` is the `ServeTlsOptions` shape
  minus client auth: iroh clients never present a certificate, so `caCerts`
  and `anyClientCert` throw. Client trust uses the `TlsClientOptions`
  semantics (extra roots on top of the web roots; a pin replaces chain and
  name checks; the two exclude each other).
- **The pin travels in the relay URL**, `?pin=<hex>`, like the token in the
  user part: the URL stays the one string that configures a client, and
  tickets carry it with no format change. The endpoint strips it (and the
  token) before iroh sees the URL. Cost: about 69 more characters in a
  self-signed relay's ticket (QR version 9 to about 11). Hex, not base64url,
  to keep one pin encoding everywhere; revisit only if QR size bites. A
  `quic=<port>` parameter can join it later for QAD.
- **`relayCaCerts`** stays an endpoint option (PEM does not fit a URL), for a
  fleet that installs a private CA once.
- **No `relay.setTls`**: no TLS server here (`serve`, `listenTls`) can swap a
  certificate, so the relay would be the odd one out. Renew by restarting
  with the new certificate; a settable token makes that cheap (clients
  reconnect on their backoff, direct paths are untouched). Renewal for all
  three is its own item.
- **`allow` is a list plus `setAllow`, not a callback.** `on_connect` runs on
  iroh's tokio tasks; a JS callback would be an engine round trip per connect
  and would only gate new connections. `setAllow` also disconnects revoked
  clients that are connected.
- **`url`, not `hostname`**: behind a TLS proxy the public scheme and port
  differ from the bound ones.
- **With `tls`, the plain listener binds `127.0.0.1:0`**: nothing reaches it
  (the probe goes to port 80). This settles the old open question.
- **A ticket's relay is adopted per connection, and the app decides the
  rest.** `connect(ticket)` adds an unknown ticket relay (with its token and
  pin) and dials through it; by default it never becomes the endpoint's home
  relay, so the endpoint's own tickets keep naming its own `relayUrl` (or
  none). An app that wants the ticket's relay as its home relay reads it with
  `parseTicket` and creates its endpoint with that `relayUrl`.
- **Out of this item**: the QAD listener (okf/backlog/p2p-relay-across-nats.md;
  it reuses the relay's `ServerConfig` and a `quic=` URL parameter), Let's
  Encrypt, certificate renewal, a runtime `setRelayUrl`.

## Surface

```ts
// flux:p2p
type RelayOptions = {
  port?: number                 // the relay's port: HTTPS with tls, else plain HTTP
  host?: string                 // bind address, default "0.0.0.0"
  tls?: { cert: string; key: string; pin?: false }  // pin: false -> Relay.url carries no ?pin=
  token?: string | string[]     // admitted tokens; the first goes into Relay.url. Default: one fresh random
  allow?: string[]              // endpoint ids, checked after the token; omit for any endpoint
  url?: string                  // public base URL (proxy, NAT, DNS name); Relay.url = it + token (+ pin)
}
class Relay {
  readonly port: number
  readonly url: string          // https://<token>@host:port/?pin=<hex> with tls
  setAllow(ids: string[] | null): void   // also disconnects connected clients no longer allowed
  clients(): string[]                    // connected endpoint ids
  close(): Promise<void>
}
type EndpointOptions = {
  ...                           // as today
  relayUrl?: string             // now also https://...?pin=<hex>; a pin on http:// throws
  relayCaCerts?: string[]       // extra roots for the relay's certificate; excludes a URL pin
}
class Endpoint {
  ...
  ticket(): Promise<string>     // now rejects (relay URL + reason) when the relay is not online after the wait
  relayStatus(): { url: string; connected: boolean; error?: string } | null
  connect(ticket, protocol)     // now adopts the ticket's relay for this connection
}
function parseTicket(ticket: string): { id: string; relayUrl: string | null; addrs: string[] }
```

## Steps

Run in order, one implementer agent each (subagent_type `implementer`, model
opus, effort medium): they share forge/src/p2p/endpoint.rs, forge/src/tls.rs
and packages/flux-types/modules/p2p.d.ts. Every step updates p2p.d.ts with
its surface (flux/CLAUDE.md "Surface changes").

1. **Relay failures become visible.** `ticket()` rejects naming the relay URL
   and the reason (`last_error` / `auth_denied_reason`) when a relay is
   configured and not online after `TICKET_WAIT`. `endpoint.relayStatus()`
   reads `home_relay_status()`. Remove the "The failure is silent" section
   from okf/backlog/p2p-relay-across-nats.md and link here.
2. **`connect(ticket)` adopts the ticket's relay.** An unknown ticket relay is
   added with the ticket's token (`insert_relay`) before dialing. The endpoint
   always has the relay transport (`RelayMode::Custom` over an empty map
   instead of `Disabled`). An adopted relay stays until the endpoint closes
   and never becomes the home relay. `parseTicket` (pure, in forge beside
   `parse_dial`) returns `{ id, relayUrl, addrs }`, `relayUrl` with its token
   and pin. Tests: Alice with a relay, Bob with none, a ticket cut to
   `id|relay|`, Bob connects from the ticket alone; afterwards Bob's
   `ticket()` and `relayStatus()` still name his own relay (or none);
   `parseTicket` round-trips token and pin.
3. **Relay options** (relay.rs, flux/src/forge_plugins/p2p/relay.rs): `host`;
   `token` (URL-safe characters, a minimum length as a named constant);
   `url`; `tls` built by `forge::tls::server_config` into
   `CertConfig::Manual` (iroh wants an owned `ServerConfig`:
   `Arc::unwrap_or_clone`; set ALPN as iroh-relay's own binary does), the
   plain listener on `127.0.0.1:0`, `Relay.url` with `?pin=` from
   `spki_sha256` of the leaf unless `pin: false`; `TokenAccess` becomes one
   access policy holding the tokens, the allowlist (behind a lock) and the
   connected set (from `on_connect`/`on_disconnect`), under `setAllow` and
   `clients`. The forge feature `p2p-relay` gains `tls`.
4. **Endpoint trust.** `relayUrl` parses `?pin=` (stripped before iroh);
   `relayCaCerts`; one verifier per endpoint through
   `CaTlsConfig::custom_server_cert_verifier`, looking the pin up by host
   from the endpoint's own `relayUrl` and from every relay step 2 adopts,
   else the web roots plus `relayCaCerts`. `forge::tls` exports its pin and
   web-root verifiers so `connectTls`, `fetch` and the relay share one code
   path and one set of error messages; the plugin reuses the parsing of
   `tls_client_options` (flux/src/forge_plugins/net.rs). The forge feature
   `p2p` gains `tls`.
5. **Tests.** forge/src/tests/p2p_relay.rs: a TLS relay with a
   `generate_certificate` certificate reached by pin and by a private CA; a
   wrong pin makes `ticket()` reject and `relayStatus` show the certificate
   error; allowed and denied ids, the denial visible in `relayStatus`;
   `setAllow` drops a live client; the second token is admitted; a loopback
   `host`; the `url` override. flux/tests/p2p.test.ts: scan and connect end
   to end against a self-signed HTTPS relay that only the ticket names, and
   the validation throws (`caCerts` in relay `tls`, a bad token, a pin on
   `http://`, a URL pin with `relayCaCerts`).
6. **Docs and notes.** p2p.d.ts documents `https://`, the pin, the adopted
   relay, and "save the relay's cert and key once". docs/runtime/index.md is
   untouched (no module or capability change). Update this note to what
   shipped and `git mv` it to okf/done/. File backlog items: certificate
   renewal for `serve`, `listenTls` and `serveRelay` (one reloadable cert
   resolver in `forge::tls`); Let's Encrypt (`CertConfig::LetsEncrypt` is
   already compiled); `endpoint.setRelayUrl(url | null)` (switch the home
   relay at runtime).
7. **Review and checks.** One review agent (model opus, effort high) reads
   the whole diff, given the code and this note but not the implementers'
   conclusions. Then `cargo test -p forge` for the p2p relay tests, `bun run
   sol test --only flux`, `bun run sol check`.

## Open

- Whether `insert_relay` can move iroh's home relay choice: step 2's test
  settles it; if it does, the home relay has to be fixed to `relayUrl`.
- The verifier sees a host, not a port: two relays on one host with
  different pins collide. Accept and document, unless it bites.
- The value of the minimum token length.

Related: okf/backlog/p2p-relay-across-nats.md (QAD, and step 1 comes from
it), okf/backlog/p2p-ticket-addresses.md (what a ticket carries beyond the
room).
