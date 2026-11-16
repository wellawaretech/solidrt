---
title: Switch an endpoint's relay at runtime
description: A p2p endpoint's `relayUrl` is fixed at `Endpoint.create`, so an app that learns its relay later (scans a ticket, gets a new relay URL from its config) or wants to drop its relay must close the endpoint and create a new one, which ends every connection; an `endpoint.setRelayUrl(url | null)` would switch the home relay in place.
created: 2026-11-16
---

# Switch an endpoint's relay at runtime

## Symptom

`EndpointOptions.relayUrl` is read once, in `Endpoint.create`
(forge/src/p2p/endpoint.rs). It decides what `ticket()` names and what
`relayStatus()` reports. To use another relay as its own, for example the
relay of a ticket it scanned (`parseTicket(ticket).relayUrl`), or a relay
URL that changed in the app's config, the app closes the endpoint and
creates a new one. That ends every open connection and stream.

`connect(ticket)` already adopts a ticket's relay for dialing, but an
adopted relay is never the endpoint's own: its tickets still name the old
`relayUrl`, and iroh may move its home relay to the adopted one (the gap in
okf/upstream/iroh-single-home-relay.md).

## Shape

- `endpoint.setRelayUrl(url | null)`: parse the URL as `create` does (token,
  `?pin=`), insert it with `insert_relay` (or keep it if already adopted),
  register its pin with the endpoint's verifier, and make it the URL that
  `ticket()` and `relayStatus()` name. `null` removes the endpoint's own
  relay (direct paths, plus adopted relays).
- The old relay: `remove_relay` it, unless a live connection runs through
  it.
- iroh picks the home relay by latency and has no public way to pin it, so
  the new relay is the home relay only if iroh prefers it. Wait for the
  upstream ask, or document that `relayStatus()` shows whether it took.
- The per-host check: a URL on a host the endpoint already checks another
  way rejects, as in `connect`.

## Open

- Whether an app needs it: the use case today is one shared relay that
  both peers configure at `create`.

Related: okf/done/p2p-relay-on-a-public-network.md.
