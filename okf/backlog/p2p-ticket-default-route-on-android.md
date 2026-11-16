---
title: Verify the p2p ticket's default-route address on Android
description: A ticket without a relay carries the IPv4 of the default-route interface, which netwatch reads; on Android it falls back to running `ip route show table 0`, unverified on an old and a current device, so an Android host may write the wrong address or fall back to every IPv4.
created: 2026-11-16
---

# Verify the p2p ticket's default-route address on Android

## Symptom

`Endpoint.ticket()` without a relay (`addresses: "auto"`, the default) carries
one address: the IPv4 of the default-route interface. forge reads that
interface through netwatch (`interfaces::State::new`,
`default_route_interface`). On Android netwatch reads `/proc/net/route`
and, when that gives nothing, runs `ip route show table 0`. Newer Android
releases restrict `/proc/net` and may refuse the subprocess or its netlink
reads. Neither path was checked on a device.

When the read fails, the ticket is not wrong but larger: with no default
route, `local_ticket_addresses` (forge/src/p2p/endpoint.rs) falls back to every
IPv4 address. A read that names the wrong interface (the mobile data
interface while Wi-Fi is up) would put an address the LAN peer cannot use
into the ticket, and the pairing would fail.

## Done looks like

- On an Android 8 device and a current phone, both on Wi-Fi with mobile
  data on, an endpoint without a relay prints a ticket with exactly the
  Wi-Fi IPv4, and a desktop dials it.
- `sol server --tunnel` on a desktop and an Android client pair by its QR
  code (the client side dials; this checks the desktop's ticket from the
  device's view).
- If netwatch cannot read the route on a release, the finding goes to
  `okf/upstream/` and the fallback (every IPv4) is documented in p2p.d.ts
  as the Android behaviour.

## What it involves

A scratch app that prints `ticket()` and `ticket({ addresses: "all" })`, run on
both devices; compare with `adb shell ip route`.

Related: okf/done/p2p-ticket-addresses.md.
