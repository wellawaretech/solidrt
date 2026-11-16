---
title: Let serveRelay run on a public network
description: Alice shows a QR code, Bob scans it, they connect through a relay, on a public network too. Shipped - the relay takes host, tls (pinned when self-signed), fixed tokens, an endpoint allowlist with setAllow and clients(), and a public url; the endpoint trusts an https relay by a ?pin= in its URL or by relayCaCerts, adopts the relay a ticket names per connection, and reports a relay that never comes online through ticket() rejecting and relayStatus().
created: 2026-11-16
completed: 2026-11-16
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

The relay decides by default whether `Relay.url` carries the pin: yes for a
self-signed leaf (issuer equals subject, what `generateCertificate` makes
without an `issuer`), no for a certificate a CA signed. A CA-signed
certificate is checked by its chain (the web roots, or `relayCaCerts` for a
private CA), and its key usually changes at every renewal: Let's Encrypt
makes a new key every 60 to 90 days unless told to reuse it, and a pinned
URL would break every QR code and config at each renewal. `tls.pin: true`
or `false` overrides the default.

The pin protects only the hop to the relay. Alice and Bob authenticate each
other end to end by endpoint id, as today; a fake relay still reads nothing,
but would see the token and the metadata.

## What iroh 1.4.1 offers

The relay server config, its access hook, the endpoint's custom verifier,
runtime relay insertion, the token lookup and the home relay choice are in
okf/notes/iroh-relay-internals.md.

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
- **A ticket's relay is adopted per connection; the endpoint's own surface
  names only its `relayUrl`.** `connect(ticket)` adds an unknown ticket relay
  (with its token and pin) and dials through it. iroh may then make it the
  home relay (see "Home relay choice" in okf/notes/iroh-relay-internals.md;
  no API prevents it), so the surface filters instead: `ticket()` writes
  only the configured `relayUrl` (`encode_ticket` used to take the first
  relay address it saw), and
  `relayStatus()` reports the `home_relay_status` entry of `relayUrl`
  (`null` without one). An adopted relay carrying traffic for the endpoint is
  fine and invisible. An app that wants the ticket's relay as its own reads
  it with `parseTicket` and creates its endpoint with that `relayUrl`.
- **The home-relay gap is accepted and documented.** iroh keeps one home
  relay, so an endpoint with its own `relayUrl` that dials a ticket naming a
  different relay may have iroh move its home relay to the adopted one:
  its own relay then drops when idle, its tickets (still naming `relayUrl`)
  may not reach it, and `relayStatus()` reports it not connected. Accepted
  because the use case is a shared relay both peers configure (the ticket's
  relay is then the endpoint's own and is never adopted), and an endpoint
  without a `relayUrl` has no gap. p2p.d.ts (`connect`) says so and
  recommends sharing the relay, or a separate endpoint created with the
  ticket's relay (`parseTicket`). There is no public iroh API to pin the
  home relay: okf/upstream/iroh-single-home-relay.md.
- **`ticket()` rejecting when the relay is down is a behaviour change**: a
  LAN app whose relay is down used to get a direct-only ticket. Decided
  anyway: a ticket that cannot be dialed is the worse failure, and
  `relayStatus()` is the observable. `ticket({ relay: false })` is the
  opt-out: a direct-only ticket, no relay wait, no relay rejection.
- **`ticket()` waits on the configured relay's own entry**, not on iroh's
  `online()`, which resolves for any connected home relay. Once an adopted
  relay was home, `online()` resolved at once and `ticket()` wrote a dead
  `relayUrl` into the ticket. When another relay is home instead, the
  rejection says so ("iroh made <url> the home relay"), and so does
  `relayStatus().error` (one reason, `status_of`, for both). `relays()`
  marks each relay `home` from iroh's home relay status, so the gap is
  observable.
- **A ticket naming a known relay with other access rejects.** The
  endpoint keeps its own record of each relay's token and pin (configured
  or adopted). iroh reads a relay's token once, when its relay connection
  starts, and `remove_relay` does not stop a running one (see
  okf/notes/iroh-relay-internals.md), so the endpoint cannot switch to a
  new token or pin. Silently keeping the old one fails later and quietly;
  rejecting names the relay (no token, no pin) and what differs, and says
  a new token or key needs a new endpoint. For any known relay, another
  pin rejects ("another pin than this endpoint checks it by"), and so does
  a pin for a relay checked by its chain ("the ticket pins it; this
  endpoint checks its certificate chain"). For an adopted relay only,
  another token rejects, and so does a token for a relay adopted without
  one ("this endpoint adopted it without a token"): the relay admits the
  endpoint's own token for its configured relay. A ticket without a token
  or without a pin supplies none and uses the known one, so a pinned relay
  stays checked by its pin. The per-host "already checked" error of the
  verifier is the same class and reads the same way.
- **No `removeRelay`**: iroh 1.4.1 `Endpoint::remove_relay` only drops the
  map entry and re-runs the net report; a running relay connection keeps
  its token and ends only after 60 s idle, and never while it is home. So
  relays are never removed or replaced; `relays()` lists them.
- **`connect(ticket, protocol, { adoptRelay: false })`** is the knob for
  the adoption default: an unknown ticket relay is not added and not
  dialed; the dial uses the ticket's relay only when the endpoint knows it,
  and the ticket's direct addresses. iroh never routes to a peer through
  the dialer's own relay unless the ticket names it, so an unknown relay
  and no direct address rejects before the dial ("Relay <url>: not known
  to this endpoint and adoptRelay is false, and the ticket carries no
  direct address", no token or pin) instead of iroh's "No addressing
  information available".
- **Adoption is cancel safe.** A dropped `connect` stops at an await. Past
  the adoption lock, the host trust, the endpoint's record and iroh's map
  entry are written in one poll (`insert_relay` writes the map before its
  own await), record first, so no map entry is ever left without a record.
  A record without an entry happens only on a closed endpoint.
- **`relayCaCerts` and a URL pin may coexist.** The verifier is a per-host
  lookup (pin by host, else roots) and handles pinned and CA relays side by
  side as soon as relays are adopted, so excluding the two on one endpoint
  would be a validation rule that buys nothing.
- **Let's Encrypt is the app's, not the runtime's.** `tls: { cert, key }`
  takes the PEMs whatever issued them: certbot, a reverse proxy that
  terminates TLS (then no `tls` at all, `host: "127.0.0.1"` and `url`), or
  any other ACME client. Built-in ACME would make the relay own port 80 or
  443 for the challenge, keep the account and the cache, and renew in the
  process; the tools above do that better and the app stays in control. So
  `CertConfig::LetsEncrypt` stays unused and no backlog item is filed for it.
- **The pin default follows the certificate** (see "The pin, briefly"): a
  self-signed leaf is pinned, a CA-signed one is not, `pin: true | false`
  overrides. A web-root certificate then needs no flag from the app.
- **A failed certificate check is reported from our own verifier.** iroh
  hides it: the net report's HTTPS probe fails first, the relay never
  becomes home, and the probe error is not public. The endpoint's
  `HostVerifier` runs inside that probe, records the last rejection per
  host and clears it on a pass; `relayStatus().error` and the `ticket()`
  rejection read it. No probe of our own. This is the second ask in
  okf/upstream/iroh-single-home-relay.md.
- **One host, one check.** The verifier sees a host, not a port. A ticket
  relay on a host the endpoint already checks another way (another pin, or
  a pin against the chain) fails `connect`, naming the relay, and is not
  adopted.
- **An HTTPS relay URL with an IPv6 host is refused at `serveRelay`.** The
  iroh-relay 1.4.1 client builds the TLS server name from the URL's
  `host_str`, brackets included, and rustls refuses it: no endpoint could
  reach such a relay. A specific IPv6 `host` with `tls` (and no `url`), or
  an `https://` `url` with an IPv6 address, throws. The unspecified `::`
  does not: its URL carries the default route's IPv4.
- **The URL host of an unspecified bind** is the default route's IPv4,
  read through netwatch as the endpoint reads it for a ticket
  (okf/done/p2p-ticket-addresses.md); without a default route the first
  IPv4 neither loopback nor link-local, and `127.0.0.1` only on a machine
  with none.
- **No error echoes a secret**: a rejected token, a `url` or `relayUrl`
  with a user part. Relay tokens are compared by their SHA-256 digests,
  every token, with no early exit.
- **`relayUrl` takes `?pin=` and nothing else as query**, once, on
  `https://` only, and no password or fragment. A URL error rejects
  `Endpoint.create` (the bind is async) instead of throwing.
- **A relay URL is `http://` or `https://` only**, as `relayUrl` and in a
  ticket. The iroh-relay 1.4.1 client dials every scheme but `http` and
  `ws` with TLS, and `HostTrust` checks only `https` relays, so a `wss://`
  relay was dialed with TLS past the endpoint's own check.
- **No `|` in a relay URL** (`relayUrl`, a ticket's relay, the relay's
  `url`): it separates a ticket's fields. A ticket with more than three
  fields is refused instead of having its tail ignored.
- **The endpoint refuses an `https://` relay URL with an IPv6 address**
  too, as `relayUrl` and in a ticket, for the reason `serveRelay` does
  (below); `http://` on IPv6 stays allowed.
- **`ticket({ relay: false })` rejects on a `paths: "relay"` endpoint**,
  at once: with no UDP socket it has no direct address, and the ticket
  would name the id alone, which nobody can dial.
- **Tokens are URL-unreserved characters, at least 16 long**
  (`MIN_TOKEN_LEN`); the default token is 64 hex. `allow: []` admits
  nobody; `null` or omitted admits any endpoint with a token. `setAllow`
  takes its argument explicitly (`null` clears; a missing one throws, so a
  forgotten argument never opens the relay). It has a narrow race: a
  client admitted just before it but not yet registered as connected
  stays until it reconnects (noted in forge/src/p2p/relay.rs).
- **Self-signed means issuer equals subject** (`forge::tls::is_self_signed`).
  A CA whose subject equals its leaf's subject reads as self-signed and is
  pinned by default; `tls.pin: false` overrides. A stricter check (the
  leaf's signature under its own key, or AKI/SKI) is not done: the
  override covers the rare case, so it stays a documented limit, on the
  helper and in p2p.d.ts.
- **The relay `host` is an IP address** (`RelayConfig` takes a
  `SocketAddr`). The `url` override is `http(s)` with a host and no user
  part, query, fragment or `|`; `http://` with `tls` throws, pinned or
  not, since clients dial an `http://` URL without TLS.
- **`Endpoint::bind` (forge) takes the relay CA certificates** as a sixth
  positional parameter; lattice/src/go/tunnel.rs passes none. `connect`
  and `connect_io` take `ConnectOptions { adopt_relay }` and `ticket`
  takes `TicketOptions { relay }`, both `Default` to `true`; the tunnel
  passes the default.
- **Out of this item**: the QAD listener (okf/backlog/p2p-relay-across-nats.md;
  it reuses the relay's `ServerConfig` and a `quic=` URL parameter),
  certificate renewal (okf/backlog/tls-certificate-reload.md), a runtime
  `setRelayUrl` (okf/backlog/p2p-set-relay-url.md).

## Surface

As shipped (packages/flux-types/modules/p2p.d.ts is the full contract):

```ts
// flux:p2p
type RelayOptions = {
  port?: number                 // HTTPS with tls, else plain HTTP; 0 or omitted for a free one
  host?: string                 // bind address, an IP; default "0.0.0.0"
  tls?: { cert: string; key: string; pin?: boolean }  // ?pin= in Relay.url; default: true for a self-signed leaf, false for a CA-signed one; caCerts / anyClientCert throw
  token?: string | string[]     // admitted tokens (>= 16 URL-unreserved chars); the first goes into Relay.url. Default: one fresh random, 64 hex
  allow?: string[]              // endpoint ids, checked after the token; omit for any, [] for none
  url?: string                  // public base URL (proxy, NAT, DNS name), http(s), no user part, query, fragment or |; http:// with tls throws
}
class Relay {
  readonly port: number         // the HTTPS port with tls
  readonly url: string          // https://<token>@host:port/?pin=<hex> with a pinned tls
  setAllow(ids: string[] | null): void   // also disconnects connected clients no longer allowed; no argument throws
  clients(): string[]                    // connected endpoint ids
  close(): Promise<void>
}
type EndpointOptions = {
  ...                           // secretKey, protocols, port, paths as before
  relayUrl?: string             // http(s) only, also https://<token>@host:port/?pin=<hex>; a pin on http://, a bad pin, a second pin, another query,
                                // a password, a fragment, a | or https:// with an IPv6 address rejects create
  relayCaCerts?: string[]       // PEM, extra roots on top of the web roots for unpinned https relays (own and adopted); not PEM rejects create
}
class Endpoint {
  ...
  ticket(opts?: { relay?: boolean; addresses?: "auto" | "all" }): Promise<string>
                                // names only relayUrl (token and pin); rejects naming the relay and the reason when its own
                                // entry is not connected after 3 s; relay: false is direct-only, no wait, no relay rejection
                                // (it rejects at once with paths: "relay", which has no direct address); addresses: see
                                // okf/done/p2p-ticket-addresses.md (none beside a relay unless it is loopback/link-local)
  relayStatus(): { url: string; connected: boolean; error?: string } | null   // the relayUrl entry (url without token and pin); null without one;
                                // error says "iroh made <url> the home relay" when another relay is home
  relays(): { url: string; pin?: string; configured: boolean; home: boolean }[]  // configured first, then adopted; home: iroh's home relay
  connect(ticket, protocol, opts?: { adoptRelay?: boolean })
                                // adopts an unknown ticket relay (token and pin) for good unless adoptRelay: false; a known
                                // relay with another pin, or a pin where its chain is checked (or an adopted one with another
                                // token, or a token where it has none), rejects, and so does a host checked another way; a
                                // ticket without a token or pin uses the known one; adoptRelay: false with an unknown relay
                                // and no direct address in the ticket rejects before the dial
}
type Ticket = { id: string; relayUrl: string | null; addresses: string[] }  // relayUrl with token and pin
function parseTicket(ticket: string): Ticket   // throws when it does not parse, or has more than 3 fields
```

## What shipped

In the order of the plan's steps:

1. Relay failures are visible: `ticket()` rejects with the relay URL
   (without its token) and the reason, `endpoint.relayStatus()` reads the
   `relayUrl` entry of `home_relay_status()` and the verifier's last
   rejection for its host.
2. `connect(ticket)` adopts the ticket's relay (`insert_relay` with token
   and pin before the dial); the endpoint always has the relay transport
   (`RelayMode::Custom` over an empty map). `encode_ticket` writes only the
   configured `relayUrl`. `parseTicket` in forge beside `decode_ticket`.
3. Relay options in forge/src/p2p/relay.rs and
   flux/src/forge_plugins/p2p/relay.rs: `host`, `token`, `allow`, `url`,
   `tls` through `forge::tls::server_config` with no ALPN, the plain probe
   listener on `127.0.0.1:0`; one access policy holds the tokens, the
   allowlist and the connected set. rustls-webpki `=0.103.16` is a direct
   forge dependency behind `tls`; `p2p` and `p2p-relay` imply `tls`.
4. Endpoint trust: `?pin=` parsed and stripped, `relayCaCerts`, one
   `HostVerifier` per endpoint over a `HostTrust` map (pin by host, else
   the web roots plus `relayCaCerts`). `forge::tls` exports `web_roots`,
   `roots_verifier`, `pin_verifier`, `leaf_from_pem` and `is_self_signed`,
   shared with `connectTls` and `fetch`.
5. Tests: forge/src/tests/p2p_relay.rs (tokens, allowlist and `setAllow`
   with and without TLS, loopback `host`, `url` override, pin default and
   override, pin and private CA trust, wrong pin, one host one check,
   validation) and flux/tests/p2p.test.ts (scan and connect through a
   pinned HTTPS relay only the ticket names, `parseTicket`, the
   validation throws and rejections, a pinned `relayUrl` beside
   `relayCaCerts`).
6. p2p.d.ts documents `https://`, the pin and how it travels, saving the
   relay's `cert` and `key`, the adopted relay, the per-host check, the
   home-relay gap and where a CA-signed certificate comes from.
   docs/runtime/index.md is unchanged (no module or capability change).
7. Review follow-ups: `ticket()` waits on the configured relay's own
   entry; the endpoint records each relay's token and pin and rejects a
   ticket that conflicts; `connect(..., { adoptRelay })`, `relays()` and
   `ticket({ relay })`; `setAllow` requires its argument and prefixes its
   errors once; the relay refuses a pin on an `http://` `url` and an
   HTTPS URL with an IPv6 host; `relayUrl` refuses a password and a
   fragment; no error echoes a token or a user part; the token compare is
   by digest. Tests in forge/src/tests/p2p_relay.rs and
   flux/tests/p2p.test.ts cover each.
8. Second review: a relay URL is `http(s)` only and has no `|`, a ticket
   has at most three fields, the endpoint refuses `https://` on IPv6;
   `adopt_relay` records before `insert_relay` (cancel safe); `ticket({
   relay: false })` rejects on a relay-only endpoint; the relay refuses an
   `http://` `url` with any `tls`; `relayStatus().error` names another home
   relay and `relays()` reports `home`; a ticket without a pin uses the
   known pin, and the conflict errors name each case; the touched errors
   are sentence-case ("Invalid relay URL: ...", "Authentication denied
   (...)"; suffixes after "Relay <url>: " and "serveRelay: <option>: " keep
   their lowercase). Tests cover each, and `adoptRelay: false` with a known
   relay.

9. Third review: `adoptRelay: false` with an unknown ticket relay and no
   direct address rejects before the dial, naming the relay without its
   token; the relay's URL host reads the default route through netwatch
   as the endpoint does, and falls back to the first non-loopback IPv4;
   a ticket naming a loopback or link-local relay carries the local
   addresses (okf/done/p2p-ticket-addresses.md); `Ticket.addrs` is
   spelled `addresses`. Tests in forge/src/tests/p2p_relay.rs and
   flux/tests/p2p.test.ts.

## Resolved

- The verifier sees a host, not a port: one host, one check; a ticket
  relay on a host checked another way fails `connect` (Decisions).
- The minimum token length is 16 (`MIN_TOKEN_LEN`).

## Follow-ups

- okf/backlog/tls-certificate-reload.md: renew the certificate of `serve`,
  `listenTls` and `serveRelay` without a restart.
- okf/backlog/p2p-set-relay-url.md: switch an endpoint's relay at runtime.
- okf/upstream/iroh-single-home-relay.md: pin the home relay, and expose
  the probe errors.
- okf/backlog/p2p-relay-across-nats.md: the QAD listener.

Related: okf/backlog/p2p-relay-across-nats.md (QAD, and step 1 comes from
it), okf/done/p2p-ticket-addresses.md (what a ticket carries beyond the
room).
