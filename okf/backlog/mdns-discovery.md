---
title: Fix mDNS discovery (Discover finds nothing)
description: The go client's mDNS browse is intact but nothing advertises _solidrt._tcp anymore since the dev server moved into flux and advertise was deliberately dropped, so Discover searches forever; restoring it means a forge::mdns responder (advertise) exposed as a flux capability, and its feasibility next to system responders is unproven.
created: 2026-08-01
---

# Fix mDNS discovery (Discover finds nothing)

Discovery is half a feature today. The client half works: `run_discover`
(lattice/src/go/connection.rs) browses `_solidrt._tcp.local.` via the mdns-sd
crate, with resolve/retry/re-browse logic, desktop only (`can_discover` is
`cfg!(not(target_os = "android"))`). The server half is gone: when the dev
server became a flux script, the Bun-side `bonjour-service` advertise was
dropped by decision, not accident (docs/flux-dev-server-plan.md: "mDNS
advertise is dropped, not ported - and no mDNS code is deleted"; the p2p
ticket became the single cross-device connect story). The advertise code
survives as a comment in packages/cli/src/dev-server.ts. So pressing Discover
put the client into "Searching..." with nothing on the LAN that could ever
answer; the launcher's Discover button was commented out on 2026-08-01
(apps/launcher/parts/connect-panel.tsx) to remove the dead end.

Note the plan explicitly accepted this gap and said "do not bolt advertise
back on to bridge it". This item is a deliberate revisit of that decision,
not an oversight fix: either advertise returns as a proper capability, or
discovery should be dismantled on the client side too instead of sitting
parked forever.

## Shape of the fix

The dropped-code comment already names the home: advertise belongs next to
the server, as a flux capability, not in the CLI. That means:

- `forge::mdns` grows a responder alongside its one-shot queries: a
  persistent 5353 multicast socket answering PTR/SRV/TXT/A queries for our
  service instance, with RFC 6762 announce on start and goodbye on stop.
  This is a real step up from the existing query helper (bind, send,
  collect for a window, correlate), which is fire-and-forget.
- `flux:mdns` exposes it (e.g. `advertise(service, port, opts)` returning a
  stop handle), keeping the plugin a thin marshalling layer.
- The dev server publishes `_solidrt._tcp` next to its serve call.
- The launcher uncomments Discover (connect-panel.tsx).

## Feasibility questions (why this may be a dead end)

- Coexistence on port 5353. Every desktop OS already runs a responder
  (Avahi, Bonjour, Windows' mDNS service). The query side shares the port
  fine, but a second *responder* on the same host enters RFC 6762 probing
  and conflict territory. The safe subset is answering only for our own
  instance records and pointing SRV at the host's existing `.local` name
  rather than defending our own A record - whether that subset works
  reliably next to all three system responders is exactly what a spike has
  to show. Registering through the system daemons instead (D-Bus/Avahi,
  dnssd) would be robust but contradicts the reason forge::mdns exists
  (no external binary, no root, engine-free).
- Who actually benefits. The local desktop client gets the address on argv;
  Android/TV cannot browse (raw multicast needs a MulticastLock; NsdManager
  is the platform-native route, and none of that is wired). The remaining
  beneficiary today is a desktop client on a different machine launched
  outside the CLI - narrow. Android browse support may need to ride along
  for the responder to pay for itself, which widens the scope further.
- The ticket flow already covers cross-device connect (iroh dials direct on
  LAN), and recents cover the repeat case. Discovery has to beat "scan the
  QR once" by enough to justify a responder implementation.

## First stage

A feasibility spike, nothing user-visible: a minimal forge::mdns responder
advertising `_solidrt._tcp` on a host that is also running its system
responder, verified against the go client's existing browse on Linux, macOS
and Windows. If that falls over, close this item as infeasible-as-designed
and decide whether to remove the client's Discover path (per the plan doc,
that code is kept as future scaffolding, so removal is its own decision).

Related: android-dev-server-persistence.md (the same "no path back from the
couch" pain that discovery would soften), docs/flux-dev-server-plan.md,
docs/flux-mdns-plan.md.
