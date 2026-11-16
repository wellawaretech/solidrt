---
title: Trim p2p tickets to the addresses a peer needs
description: A p2p ticket listed every interface address (VPN, container bridge, LAN, global IPv6). Shipped - a relay ticket carries no address (beside a loopback or link-local relay, the local one), a ticket without a relay only the default-route IPv4 (every IPv4 without a default route, never IPv6), and ticket({ addresses: "all" }) writes every address as before; sol server --tunnel-addresses all is the tunnel's opt-out.
created: 2026-09-14
completed: 2026-11-16
---

# Trim p2p tickets to the addresses a peer needs

`Endpoint.ticket()` put every direct address iroh knows into the ticket. On a
desktop with a WireGuard tunnel, Docker and IPv6, a local ticket read:

```
<id>||10.8.0.2:50123,172.17.0.1:50123,192.168.1.20:50123,[2001:db8::1]:50124
```

`encode_ticket` copied `EndpointAddr.addrs` unfiltered. iroh collects them
from every non-loopback interface, pairs each with the bound port, and
orders them numerically (a `BTreeSet`), so the LAN address gets no
priority. A peer on the LAN can use exactly one of them.

Two costs:

- **It exposes the network layout.** The ticket shows the VPN and its address,
  that a container runtime is installed, the LAN subnet, and the global IPv6
  address, which is routable, long-lived and carries the provider's prefix. A
  QR code on a screen reaches the room, but a ticket is a string made to be
  passed on (a link, a chat message, a screenshot), and with relay support it
  travels further. iroh's own publisher avoids this by default:
  `PkarrPublisher` uses `AddrFilter::relay_only`, "This avoids leaking IP
  addresses to the public pkarr server". Browsers fixed the same leak in WebRTC
  by replacing local host candidates with random mDNS names.
- **It doubles the QR code.** With a full global IPv6 address the ticket above
  is about 165 characters, QR version 9 at level M (53x53 modules). The id plus
  one LAN IPv4 is at most 87, version 6 (41x41).

`sol server --tunnel` prints a local ticket as its pairing QR code
(`packages/cli/src/server/tunnel.ts`), so the dev server had both costs.

## Shape

- **Relay tickets: id and relay URL, no addresses.** iroh dials through the
  relay, the peers exchange their direct addresses inside the encrypted
  connection (n0 QUIC NAT traversal), and the connection moves to a direct
  path. The addresses then reach only a peer that connects. The costs: the
  first packets go through the relay until the direct path is up, and the
  relay is a hard dependency. A peer that cannot reach the relay cannot
  connect, even on the same LAN; before, the direct addresses covered that.
  `addresses: "all"` is the opt-out, and it also brings back a portmapped
  (UPnP, PCP) public address when iroh reports one.
- **A relay only this machine reaches: the local addresses too.** A relay
  URL whose host is `localhost` or a loopback, link-local or unspecified IP
  (`serveRelay({ host: "127.0.0.1" })`, its `url` as `relayUrl`) cannot
  serve another machine, so a ticket naming it alone was undialable from
  elsewhere (`id|http://tok@127.0.0.1:port/|`). Beside such a relay the
  ticket carries what a local ticket does.
- **Local tickets: one address, the IPv4 of the default-route interface.**
  Without a relay the ticket is the only carrier of addressing, so one
  address has to be in it. A private IPv4 says nothing beyond what is true
  of most home networks.
- **No default route: the IPv4 addresses only, never IPv6.** Rare, since
  DHCP hands out a gateway even when the router has no uplink.

Where the local rule picks wrong, accepted:

- A full-tunnel VPN may own the default route, and the ticket then
  carries the VPN address. Linux wg-quick and Tailscale use policy
  routing, so the main table's default route usually stays on the LAN.
- With two LANs on different subnets (Ethernet and Wi-Fi) only one is exposed.
- An IPv6-only LAN is not covered (the endpoint binds IPv4 only anyway).

## Decisions

- **The knob is `ticket({ addresses: "auto" | "all" })`**, beside the
  existing `relay` option of `TicketOptions` (forge
  `TicketAddresses::{Auto, All}`). Spelled out, as `parseTicket`'s
  `addresses` is; the older abbreviated surfaces are
  okf/backlog/spell-out-addrs.md.
  `"auto"` is the default and the rule above; `"all"` writes every direct
  address, beside a relay too, as before. Any other value throws. The
  ticket is the observable: `parseTicket` shows its `addresses`.
- **The choice is a pure function over plain values**, not over netwatch's
  `State`: netwatch's `Interface` has private fields and its `fake()` is
  crate-private, so only `State::fake()` (one Wi-Fi interface) can be built
  outside netwatch. `local_ticket_addresses(addrs, default_route_ips)` picks;
  `default_route_ips(&State)` reads the default-route interface's IPs and
  is tested on `State::fake()` (present, missing interface, no route).
  forge/src/tests/p2p_ticket.rs drives the picker with a VPN, a container
  bridge, the LAN and a global IPv6.
- **A default route whose IPv4 is not among iroh's addresses** (an
  IPv6-only default route, or an address iroh did not report) counts as
  no default route: every IPv4. A ticket that names nobody is the worse
  failure. Every entry that matches an IPv4 of that interface is kept (an
  interface with a secondary IPv4 writes both).
- **The interface state is read per `ticket()` call** (`State::new`, a
  netlink read on Linux), only for a ticket without a relay and with
  `"auto"`. Tickets are rare; no cache.
- **netwatch `=0.19.3` is a direct forge dependency behind `p2p` and
  `p2p-relay`**, the version iroh 1.4.1 uses; it moves with iroh. The
  relay reads the default route through it too (`default_route_ips` in
  forge/src/p2p.rs, shared), not through netdev, so its URL and a ticket
  never disagree on the default route. Without a default route the
  relay's URL names the first IPv4 that is neither loopback nor
  link-local (`relay_host_ipv4`), and
  `127.0.0.1` only on a machine with none.
- **"Local relay" is a pure check of the URL host** (`local_relay`): a
  DNS name other than `localhost` and the names under it (RFC 6761, a
  trailing dot too) counts as reachable, even when it resolves to
  loopback; resolving it per ticket is not worth it.
- **`sol server --tunnel-addresses <auto|all>`** passes `addresses` to the
  tunnel's ticket (default `auto`; it needs `--tunnel`, another value
  fails). The go client only dials, so lattice/src/go/tunnel.rs is
  unchanged.

## What shipped

- forge/src/p2p/endpoint.rs: `TicketOptions.addresses`, `TicketAddresses`
  with `parse`, `local_relay`, `local_ticket_addresses`; `encode_ticket`
  takes the chosen addresses; `Ticket.addresses`. `default_route_ips` in
  forge/src/p2p.rs, shared with the relay (forge/src/p2p/relay.rs
  `relay_host_ipv4`). Doc comments on `Endpoint::bind` and `ticket`.
- flux/src/forge_plugins/p2p/endpoint.rs: `ticket` reads `addresses` and
  throws on another value; `parseTicket` returns `addresses`.
- packages/flux-types/modules/p2p.d.ts: `TicketOptions.addresses` (the
  relay dependency, the local relay, `"all"` as the opt-out), the
  `EndpointOptions` header and `ticket()` say what a ticket carries.
- `sol server --tunnel-addresses` (packages/cli/src/server/args.ts, the
  usage, packages/cli/src/server/docs.md, packages/cli/agents/debugging.md,
  docs/guides/dev-loop.md).
- The tunnel header comments (packages/cli/src/server/tunnel.ts,
  lattice/src/go/tunnel.rs). The callers (`sol server --tunnel`, the go
  client's tunnel, examples/call, packages/core/tests/record.test.tsx) take
  the default.
- Tests: forge/src/tests/p2p_ticket.rs (the pure choice, the state read,
  the option parse, the local relay hosts); forge/src/tests/p2p_relay.rs (a
  loopback relay ticket carries the local address and dials without the
  relay, the relay URL host without a default route, a relay ticket
  carries no address, two endpoints connect through it and `conn_info` reports a
  direct path after; `"all"` writes the addresses beside a relay; a
  direct-only ticket is IPv4 only); flux/tests/p2p.test.ts (the same over
  JS, and the throw). Tests that prove a path through a loopback relay
  cut its ticket to `id|relay|`.
- The old bullet "No relay wait on a local endpoint" was already resolved
  by okf/done/p2p-relay-on-a-public-network.md: an endpoint without a
  relay waits only for its first direct address (1 ms measured on a
  desktop).

## Follow-ups

- okf/backlog/p2p-ticket-default-route-on-android.md: verify netwatch's
  default-route read on an Android 8 device and a current phone.
- okf/backlog/mdns-discovery.md: mDNS could take the address out of local
  tickets as well, since a responder answers only with the addresses of
  the interface the query arrived on (RFC 6762, section 6.2).
