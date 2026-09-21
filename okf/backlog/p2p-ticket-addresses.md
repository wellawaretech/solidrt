---
title: Trim p2p tickets to the addresses a peer needs
description: A p2p ticket lists every interface address (VPN, container bridge, LAN, global IPv6), which exposes the network layout to anyone who sees it and doubles the QR code; relay tickets should carry no addresses and local tickets only the default-route IPv4.
created: 2026-09-14
---

# Trim p2p tickets to the addresses a peer needs

`Endpoint.ticket()` puts every direct address iroh knows into the ticket. On a
desktop with a WireGuard tunnel, Docker and IPv6, a local ticket reads:

```
<id>||10.8.0.2:50123,172.17.0.1:50123,192.168.1.20:50123,[2001:db8::1]:50124
```

`encode_ticket` in `forge/src/p2p.rs` copies `EndpointAddr.addrs` unfiltered.
iroh collects them from every non-loopback interface, pairs each with the
bound port, and orders them numerically (a `BTreeSet`), so the LAN address gets
no priority. A peer on the LAN can use exactly one of them.

Two costs:

- **It exposes the network layout.** The ticket shows the VPN and its address,
  that a container runtime is installed, the LAN subnet, and the global IPv6
  address, which is routable, long-lived and carries the provider's prefix. A
  QR code on a screen reaches the room, but a ticket is a string made to be
  passed on (a link, a chat message, a screenshot), and with relay support it
  will travel further. iroh's own publisher avoids this by default:
  `PkarrPublisher` uses `AddrFilter::relay_only`, "This avoids leaking IP
  addresses to the public pkarr server". Browsers fixed the same leak in WebRTC
  by replacing local host candidates with random mDNS names.
- **It doubles the QR code.** With a full global IPv6 address the ticket above
  is about 165 characters, QR version 9 at level M (53x53 modules). The id plus
  one LAN IPv4 is at most 87, version 6 (41x41).

`srt server --tunnel` prints a local ticket as its pairing QR code
(`packages/cli/src/server/tunnel.ts`), so the dev server has both costs.

## Shape

- **Relay tickets: id and relay URL, no addresses.** iroh dials through the
  relay, the peers exchange their direct addresses inside the encrypted
  connection (n0 QUIC NAT traversal), and the connection moves to a direct
  path. iroh's test `endpoint_two_relay_only_becomes_direct` covers two
  endpoints on one network. The addresses then reach only a peer that
  connects. The cost: the first packets go through the relay until the direct
  path is up, which is the detour today's addresses in relay tickets skip.
- **Local tickets: one address, the IPv4 of the default-route interface.**
  Without a relay the ticket is the only carrier of addressing, so one address
  has to be in it. netwatch reports that interface
  (`interfaces::State::default_route_interface`, a key into
  `State::interfaces`); keep the entry of `addr()` whose IP is that
  interface's IPv4. A private IPv4 says nothing beyond what is true of most
  home networks.
- **No default route: the IPv4 addresses only, never IPv6.** Rare, since DHCP
  hands out a gateway even when the router has no uplink.
- **No relay wait on a local endpoint.** `ticket()` waits for `online()` with
  a 3 s timeout. With the relay disabled `online()` never resolves, so every
  local ticket takes the full 3 s. `srt server --tunnel` awaits it before it
  starts listening (`packages/cli/src/server/main.ts`).

Where the local rule picks wrong, accepted:

- A full-tunnel VPN owns the default route, so the ticket carries the VPN
  address.
- With two LANs on different subnets (Ethernet and Wi-Fi) only one is exposed.
- An IPv6-only LAN is not covered.

## What it involves

- `forge/src/p2p.rs`: `ticket()` and `encode_ticket` follow the endpoint's
  mode. The address choice is a pure function over the interface state and
  `addr()`, so a test in `forge/src/tests/` can drive it with fake interfaces
  (netwatch has `State::fake()`). There are no p2p tests today.
- A direct `netwatch` dependency in forge, pinned to the version iroh uses
  (`=0.19.3` for iroh 1.1.0). An iroh bump moves both.
- netwatch reads the default route on Linux, Android, macOS and Windows. On
  Android it falls back to running `ip route show table 0` when
  `/proc/net/route` gives nothing; verify it on an Android 8 device and a
  current phone.
- Docs: the `local` and `ticket()` comments in
  `packages/flux-types/modules/p2p.d.ts`, `Endpoint::bind` and `ticket` in
  `forge/src/p2p.rs`, and the header comment of the tunnel. The `id|relay|ips`
  format and `parse_dial` stay; they already accept an empty address list.

## Done looks like

- On a machine with a VPN, a container bridge and IPv6, a local ticket carries
  exactly the LAN IPv4 and returns without a delay.
- A relay ticket carries no addresses; two endpoints on one LAN connect through
  it and `connInfo` reports a direct path shortly after.
- `srt server --tunnel` starts without the 3 s wait, its QR code is smaller,
  and a desktop and an Android client on the LAN still pair by it.

Later: mDNS could take the address out of local tickets as well, since a
responder answers only with the addresses of the interface the query arrived
on (RFC 6762, section 6.2). That depends on the responder in [[mdns-discovery]].
