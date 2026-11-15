---
title: Let two peers on different networks find a direct path
description: The self-hosted relay (`serveRelay`) runs plain HTTP with no QUIC address-discovery listener, so endpoints learn no public address from it and two peers behind different NATs stay on the relay for the whole call; a TLS QUIC listener would fix that at the cost of a certificate the clients must accept; whether `sol run` serves a relay for its dev clients is undecided too.
created: 2026-11-14
---

# Let two peers on different networks find a direct path

## Symptom

`serveRelay` in `flux:p2p` (forge/src/p2p/relay.rs, built in
okf/done/video-encode.md step 7) serves iroh-relay over plain HTTP. On
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

- A QAD listener on the relay. QAD is QUIC, so it needs TLS. A
  self-hosted relay has no public certificate, so it would serve a
  self-signed one, and the clients would skip verification for that relay
  only (the relay's token in its URL already authenticates who may use
  it). iroh-relay's server half has the listener; the client side needs a
  verifier override.
- The endpoint config then carries the QAD port again, for relays that
  serve one.

## Open

- Whether anyone needs this yet: the only consumer, the call, runs on one
  network. Measure a two-network call over the relay first.
- Whether `sol run` should serve a relay for its dev clients (so a call
  between two dev devices needs no host app), and on which port.

Related: okf/backlog/p2p-ticket-addresses.md (what a ticket carries once
it travels beyond the room).
