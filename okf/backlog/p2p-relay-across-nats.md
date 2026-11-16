---
title: Let two peers on different networks find a direct path
description: The self-hosted relay (`serveRelay`) has no QUIC address-discovery listener, so endpoints learn no public address from it and two peers behind different NATs stay on the relay for the whole call; a QUIC listener on the relay's TLS certificate (pinned or CA-checked, as the HTTPS relay already is) would fix that; whether `sol run` serves a relay for its dev clients is undecided too.
created: 2026-11-14
---

# Let two peers on different networks find a direct path

## Symptom

`serveRelay` in `flux:p2p` (forge/src/p2p/relay.rs, built in
okf/done/video-encode.md step 7) serves iroh-relay over plain HTTP, or
HTTPS with its `tls` option (okf/done/p2p-relay-on-a-public-network.md). On
one local network that is all a call needs: the peers find their direct
path through their own local addresses, and the relay carries what that
path does not.

Across two networks, each behind a NAT, it is not. iroh learns an
endpoint's public address through the relay's QUIC address-discovery
(QAD) listener. Ours has none: the endpoint config is built as
`RelayConfig::new(url, None)` (forge/src/p2p/endpoint.rs) because a QAD
port with nothing behind it costs 3 s of probe timeouts before the
endpoint is online. So neither peer learns its public address, no hole
is punched, and every packet of the call goes through the relay: more
delay, and all of both peers' traffic on the relay host's uplink.

## Shape

- A QAD listener on the relay. QAD is QUIC, so it needs TLS. The HTTPS
  relay already has a certificate, and the endpoint already checks it by
  the `?pin=` in the relay URL or by `relayCaCerts`, through its own
  verifier (`CaTlsConfig::custom_server_cert_verifier`), so QAD reuses the
  relay's `ServerConfig` and the same check. The QAD port can travel as a
  `quic=<port>` URL parameter beside the pin. iroh-relay's server half
  has the listener.
- The endpoint config then carries the QAD port again, for relays that
  serve one.

A configured relay that never comes online used to fail silently; that
fix (`ticket()` rejects, `endpoint.relayStatus()`) shipped in
okf/done/p2p-relay-on-a-public-network.md (step 1).

## Open

- Whether anyone needs this yet: the only consumer, the call, runs on one
  network. Measure a two-network call over the relay first.
- Whether `sol run` should serve a relay for its dev clients (so a call
  between two dev devices needs no host app), and on which port.
- With `paths: "relay"`, whether `Endpoint.create` should fail fast when
  the relay does not come online, instead of resolving to an endpoint that
  reaches nobody (today only `ticket()` rejects and `relayStatus()` says
  so). Left open when the silent failure was fixed.

Related: okf/done/p2p-ticket-addresses.md (what a ticket carries once
it travels beyond the room), okf/done/p2p-relay-on-a-public-network.md
(the relay options and the endpoint's trust options for a relay beyond
one network).
