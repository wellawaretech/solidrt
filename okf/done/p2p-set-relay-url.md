---
title: Switch an endpoint's relay at runtime
description: Shipped - `endpoint.setRelayUrl(url)` (flux:p2p) switches an endpoint's own relay in place, where the app closed the endpoint and created a new one, ending every connection; a known relay keeps its token and pin, the old one leaves iroh's map and stays home only until the new one answers, open connections keep running. Dropping the relay (`null`) is not offered: iroh 1.4.1 never clears a home relay.
created: 2026-11-16
completed: 2026-11-17
---

# Switch an endpoint's relay at runtime

## What shipped

- `forge::p2p::Endpoint::set_relay_url` (forge/src/p2p/endpoint.rs). The
  relays an endpoint knows are one record (`Relays`: configured, adopted,
  dropped), shared by `ticket`, `relay_status`, `relays` and
  `adopt_relay`; `adopting` serializes adoption and the switch.
- `connect` re-adopts a dropped relay with its known access; with
  `adoptRelay: false` a dropped relay is unknown.
- `RelayAccess::conflict` names what supplied the relay ("the ticket",
  "the URL"). A token on a relay the endpoint uses without one now reads
  "this endpoint uses it without a token" (was "adopted it", wrong for a
  configured relay).
- `setRelayUrl(url): Promise<void>` on `Endpoint` (flux:p2p), with its
  contract in packages/flux-types/modules/p2p.d.ts. Anything but a string
  throws: "setRelayUrl: takes a relay URL; an endpoint cannot drop its
  relay (use ticket({ relay: false }) to stop naming it)".
- Tests: four in forge/src/tests/p2p_relay.rs (the switch, with a
  connection through the old relay still carrying data both ways on
  relay-only endpoints; a known relay's token kept, also when dropped and
  re-adopted; a dead new relay leaving the old one home; one check per
  host), one in flux/tests/p2p.test.ts (the JS surface).

## Symptom

`EndpointOptions.relayUrl` is read once, when the endpoint binds
(`Endpoint::bind`, forge/src/p2p/endpoint.rs). It decides what `ticket()`
names and what `relayStatus()` reports. To use another relay as its own,
the app closes the endpoint and creates a new one. That ends every open
connection and stream.

`connect(ticket)` already adopts a ticket's relay for dialing, but an
adopted relay is never the endpoint's own: its tickets still name the old
`relayUrl`, and iroh may move its home relay to the adopted one (the gap in
okf/upstream/iroh-single-home-relay.md).

## Use cases

- An endpoint without a relay scans a ticket and makes the ticket's relay
  its own, so that its own tickets name it:
  `setRelayUrl(parseTicket(ticket).relayUrl)`.
- The app's config names another relay (a new host, a new relay service).

## Shape

`endpoint.setRelayUrl(url): Promise<void>`. It resolves once the switch is
made, not when the new relay connects: `ticket()` waits for that, and
`relayStatus()` reports it.

- The URL parses as `create` parses it (token, `?pin=`). From then on it is
  the relay that `ticket()` and `relayStatus()` name, and `relays()` lists
  it first, as `configured`.
- A new relay: its host goes into the endpoint's trust, then it is
  `insert_relay`ed. A relay on a host that the endpoint checks another way
  rejects, as in `connect`.
- A known relay (the current one, one adopted by `connect`, one that
  `setRelayUrl` dropped before) keeps the token and pin it was first used
  with. Another token or pin rejects (`RelayAccess::conflict`, with the
  token checked: the running relay connection keeps the token it started
  with). The same relay with the same access is a no-op. So `setRelayUrl`
  does not rotate a token or a key on one URL. That needs a new endpoint,
  as for `connect` (and see okf/done/tls-certificate-reload.md).
- The old relay always leaves iroh's map (`remove_relay`), and the
  endpoint keeps its access record: its actor can still run with its
  token, and the host trust never forgets a host's pin. A later ticket
  that names it re-adopts it with that access (or rejects another one).
  With `adoptRelay: false` it is unknown, as it is not in the map.
- The map writes are cancel-safe: iroh's `insert_relay` and `remove_relay`
  both write the map before their first await, so joining the two writes
  both maps in one poll. The records go first, in the same poll.
- A `ticket()` call that started before the switch names the relay it
  started with.

## How the switch behaves on iroh 1.4.1

- Out of the map, the old relay A is out of the latency race: the net
  report picks its preferred relay from the relays it probed this time, and
  the hysteresis that keeps the old home applies only when the old home has
  a latency in that report (iroh-1.4.1/src/net_report.rs ~796-813). So with
  no adopted relays, the new relay B becomes the home relay at the first
  report that B answers. With adopted relays, iroh picks the fastest of B
  and them: the gap in okf/upstream/iroh-single-home-relay.md, the same as
  for an endpoint created with B.
- Until B answers, A stays the home relay: iroh fills a report without a
  preferred relay with the current home (src/socket.rs ~1980), also when it
  is no longer in the map. `relayStatus()` then reports B not connected
  with "iroh made A the home relay". That is the observable.
- A live connection through A keeps running. `remove_relay` does not stop
  A's actor, which keeps its token, and iroh's 5 s path keepalive
  (src/endpoint/quic.rs:157) keeps the actor busy, so it does not exit
  while a connection uses it. It exits 60 s after it is neither the home
  relay nor used.
- Tickets handed out before the switch name A. They stop reaching the
  endpoint when A's actor exits. The app hands out new tickets.

## Not in scope: dropping the relay (`null`)

iroh 1.4.1 never clears a home relay. A report without a preferred relay
keeps the current home (src/socket.rs ~1980), a map that becomes empty runs
no report at all (`re_stun`, src/socket.rs ~1773), and the home relay's
actor never exits when idle (src/socket/transports/relay/actor.rs:524).
So after `setRelayUrl(null)` with no adopted relays, the endpoint stays
connected to the old relay forever: it is still reachable through old
tickets, and it reconnects to the relay with backoff if the relay goes
away. That is the third ask in okf/upstream/iroh-single-home-relay.md.

`ticket({ relay: false })` already stops the endpoint from naming its relay.
What `null` would add is to stop talking to it, which iroh cannot do yet.

Related: okf/done/p2p-relay-on-a-public-network.md,
okf/notes/iroh-relay-internals.md.
