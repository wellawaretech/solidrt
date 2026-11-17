---
title: iroh 1.4.1 relay internals - server config, access, trust, home relay
description: What iroh and iroh-relay 1.4.1 offer for a self-hosted relay and how the endpoint side behaves: the relay server's bind and TLS config, its access hook, the endpoint's custom verifier, runtime relay insertion and the token lookup, home relay status and choice, and why a failed certificate check is not visible through iroh.
created: 2026-11-16
---

# iroh 1.4.1 relay internals

Read while making `serveRelay` and the endpoint's relay trust
(okf/done/p2p-relay-on-a-public-network.md). Paths are under
`~/.cargo/registry/src/*/`.

- One rustls (0.23.46) across the tree, so `forge::tls` configs hand straight
  to iroh.
- Relay server (iroh-relay-1.4.1/src/server.rs): `RelayConfig::new(addr)` takes
  the bind address. `RelayConfig::tls = Some(TlsConfig::new(https_addr,
  CertConfig::Manual { server_config }))` adds an HTTPS listener and keeps the
  plain one (`http_bind_addr`), whose only job then is the captive-portal
  probe. Clients send that probe to `http://<host>/generate_204`, always port
  80 (iroh-1.4.1/src/net_report/reportgen.rs:623), never our port. So with
  TLS the plain listener can bind `127.0.0.1:0`. `CertConfig::LetsEncrypt`
  is there too (the `server` feature already pulls `tokio-rustls-acme`); we
  do not use it.
- TLS on the relay: the iroh-relay binary's Manual config is
  `with_single_cert` only, no ALPN (src/main.rs:621-642); the relay client
  offers none either. iroh wants an owned `ServerConfig`
  (`Arc::unwrap_or_clone`). With `tls`, `Server::https_addr()` is the
  relay's port and `http_addr()` the probe listener's.
- Access (server.rs ~190-300): `AccessControl::on_connect(&ClientRequest)` sees
  the token (`auth_token()`) and the endpoint id the relay handshake proved
  (`endpoint_id()`); `on_disconnect(endpoint_id, connection_id)` pairs with
  it. It runs on iroh's tokio tasks. `server.relay_service().clients()
  .disconnect(id, None)` (server/clients.rs:181) drops a live client.
- Endpoint trust (iroh-relay-1.4.1/src/tls.rs):
  `Endpoint::builder().ca_tls_config(CaTlsConfig::custom_server_cert_verifier(..))`
  takes our own verifier; no `test-utils` feature needed. It is endpoint-wide
  (every relay), and a rustls verifier sees the server name but not the port.
- Relay list at runtime (iroh-1.4.1/src/endpoint.rs:998):
  `endpoint.insert_relay(url, Arc<RelayConfig>)` / `remove_relay`. `remove_relay` only removes the map entry and re-runs
  the net report (`handle_relay_map_change` in src/socket.rs); it does not
  stop a running active-relay actor, which keeps the token it started with
  and exits only after `RELAY_INACTIVE_CLEANUP_TIME` (60 s) idle, and never
  while it is the home relay. So a relay cannot be replaced with a new
  token or pin by removing and inserting it: forge never removes a relay,
  and a ticket naming a known relay with another token or pin rejects.
  `RelayMode::Disabled` means no relay transport at all (endpoint.rs:158);
  `RelayMode::Custom` over an empty map keeps the transport for relays
  inserted later.
- `insert_relay` writes the map entry synchronously, then awaits sending
  `RelayMapChange` to the socket actor (src/socket.rs:1235): a dropped
  caller has the entry in the map already, maybe without the net report
  re-run.
- The relay client dials TLS for every scheme but `http` and `ws`
  (`use_tls`, iroh-relay-1.4.1/src/client/tls.rs:98) and websockets as
  `wss` for every scheme but those two (src/client.rs:272). So a `wss://`
  or `ftp://` relay URL is dialed with TLS; forge accepts only `http` and
  `https`, the two its trust check knows.
- The token of a relay is read from the map once, when its active-relay
  actor starts (src/socket/transports/relay/actor.rs ~1344); an actor
  already running for that URL keeps the token it started with. So a relay
  has to be inserted with its token before the first dial through it.
- Status (endpoint.rs:1406, 1922): `home_relay_status()` is a watcher of
  `Vec<RelayStatus>`, each `{ url, is_connected(), last_error(),
  auth_denied_reason() }`; `auth_denied_reason` covers a token or allowlist
  denial. There is at most one entry (one home relay, below).
- Home relay choice (src/socket.rs:1235, 1763; src/net_report.rs:749-817):
  `insert_relay` sends `RelayMapChange`, which re-runs the net report over
  the whole map; the report's `preferred_relay` becomes the home relay. It
  is the lowest-latency relay, with hysteresis: the old one stays unless the
  new one is at least a third faster. There is no public way to pin the home
  relay. So a relay added at runtime can become the home relay: always, for
  an endpoint that had none. See okf/upstream/iroh-single-home-relay.md.
- Only relays in the current report are candidates (net_report.rs
  ~796-813): the hysteresis applies when the old home has a latency in
  this report. So a home relay removed from a map that still holds another
  relay gives way at the first report that relay answers.
- A home relay is never cleared: a report without a preferred relay gets
  the current home filled in (src/socket.rs ~1980), even when it left the
  map; an empty map runs no report (`re_stun`, src/socket.rs ~1773); the
  home relay's actor never exits when idle (actor.rs:524). Third ask in
  okf/upstream/iroh-single-home-relay.md.
- iroh's QUIC config keeps every path alive (`HEARTBEAT_INTERVAL`, 5 s,
  src/endpoint/quic.rs:157), so a relay actor that a connection uses stays
  busy and does not hit its 60 s idle exit.
- A relay whose certificate fails the check never becomes home: the net
  report's HTTPS probe (`run_https_probe` in reportgen.rs) fails first, no
  actor starts, and `home_relay_status()` has no entry and no `last_error`
  for it. The probe error is not public. A custom verifier runs inside that
  probe, so it is the one place that sees the rejection; forge's
  `HostVerifier` records it per host for `relayStatus()`. Second ask in
  okf/upstream/iroh-single-home-relay.md.
- `online()` resolves when any home relay entry is connected, an adopted
  relay too, so `ticket()` waits for the configured relay's own entry in
  `home_relay_status()` instead.
- An HTTPS relay URL with an IPv6 host is unreachable: the iroh-relay
  1.4.1 client builds the TLS `ServerName` from the URL's `host_str`,
  brackets included (`[::1]`), and rustls refuses that name. `serveRelay`
  refuses such a URL (an IPv6 `host` with `tls`, or an `https://` `url`
  with an IPv6 address).
