---
title: iroh keeps one home relay and a relay added for one dial can take it over
description: iroh 1.4.1 has a single home relay, picked by the net report's preferred_relay; insert_relay re-runs the report, so a relay added only to dial one peer can become the home relay and the configured one drops when idle. No public API pins the home relay or keeps several. We accept and document the gap; the ask is a way to pin the home relay or mark a relay dial-only. A second ask: the net report's probe errors (a failed HTTPS probe's TLS error) are not observable; we read them from our own verifier.
project: iroh (github.com/n0-computer/iroh)
versions: iroh 1.4.1, iroh-relay 1.4.1
status: unfiled
link:
created: 2026-11-16
---

# iroh keeps one home relay and a relay added for one dial can take it over

## What happens

An endpoint is created with one custom relay, A (`RelayMode::Custom`).
Its tickets name A, and peers reach it through A. To dial a peer whose
address names a different relay, B, which needs an auth token, the
endpoint adds B with `Endpoint::insert_relay(B, config)`. The token has
to be in the relay map before the dial, because the active-relay actor
reads it once, when it starts.

After that, iroh may make B the home relay, and the endpoint stops
being reachable through A:

- iroh keeps one home relay. `HomeRelayWatch` holds a single
  `Option<RelayStatus>` (src/socket/transports/relay/actor.rs ~1071), so
  `home_relay_status()` has at most one entry.
- `insert_relay` sends `RelayMapChange`, which re-runs the net report
  over the whole map (src/socket.rs ~1235, ~1763). The report's
  `preferred_relay` becomes the home relay: the lowest-latency relay,
  with hysteresis (the old one stays unless the new one is at least a
  third faster; src/net_report.rs ~749-817).
- The actor of a relay that is no longer home exits after
  `RELAY_INACTIVE_CLEANUP_TIME` (60 s) without traffic (actor.rs ~524).
- No public API pins the home relay, keeps several home relays, or adds
  a relay that is only for dialing.

So a relay added to reach one peer can take the home role from the
relay the endpoint's own address names. Once A's actor exits, peers that
dial the endpoint's address through A may not reach it, and A's status
is no longer reported.

Reproduce: two relays on one machine, A with a measurably higher latency
than B (or an endpoint with no relay, which always adopts B). Endpoint X
uses A. Endpoint Y uses B. X calls `insert_relay(B)` and connects to Y
through B. `home_relay_status()` on X now names B.

## What it costs us

Our endpoint has an app-chosen relay, and `connect` adds the relay a
peer's ticket names, with its token. When the two relays differ, the
endpoint can lose its own relay as described above. Its tickets still
name the configured relay, and its status for that relay reads not
connected.

## Our workaround

None in code. We document the gap: an app with its own relay shares
that relay with its peers, so a ticket's relay is the endpoint's own and
is never added. Or it dials from a separate endpoint created with the
ticket's relay. An endpoint without a relay of its own is not affected:
taking the adopted relay as home is harmless there.

## What we would ask upstream

A way to keep the home relay where the application put it, either of:

1. A pinned home relay: a builder or endpoint option naming the relay
   that stays home, whatever the net report prefers. An error status if
   it is down, never a silent switch.
2. A dial-only relay: a flag on `insert_relay` (or on `RelayConfig`)
   saying that the relay is for reaching peers that name it and is never
   chosen as `preferred_relay`.

Either one would let an endpoint dial peers on other relays and still be
reachable on its own.

## A second ask: net report probe errors

A relay whose TLS certificate fails the check never becomes the home
relay: the net report's HTTPS probe (src/net_report/reportgen.rs,
`run_https_probe`) fails, the relay gets no latency, and no relay actor
starts. So `home_relay_status()` has no entry for it and `last_error()`
has nothing to report. The probe's error (here the TLS error) is not
observable through the public API; `Report` exposes latencies only.

Our workaround: the endpoint's own certificate verifier
(`CaTlsConfig::custom_server_cert_verifier`) records the last rejection
per host, and our relay status reads it from there. The ask: expose the
last probe failure per relay, e.g. on the net report or as a status
entry for a relay that never connected.
