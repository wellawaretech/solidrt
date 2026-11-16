---
title: Spell out addrs in the flux module surfaces
description: Three flux module surfaces abbreviate "addresses" as `addrs` (and one `addr`), where the newer p2p ticket surface says `addresses` (ticket options, parseTicket). Renaming them breaks apps that read them, so it waits for a deliberate break.
created: 2026-11-16
---

# Spell out addrs in the flux module surfaces

## Symptom

The p2p ticket surface spells the word out: `ticket({ addresses })` and
`parseTicket(..).addresses` (okf/done/p2p-ticket-addresses.md). Three older
surfaces abbreviate it, so one module now says both `addrs` and
`addresses`:

- flux:p2p `connInfo()`: `ConnInfo.addrs` (a `ConnAddr[]`), and
  `ConnAddr.addr` in each entry. packages/flux-types/modules/p2p.d.ts;
  built in forge/src/p2p/endpoint.rs (`impl From<ConnInfo> for Value`, and
  `AddrEntry`).
- flux:mdns: a resolved service's `addrs` (the A/AAAA addresses).
  packages/flux-types/modules/mdns.d.ts; built in forge/src/mdns.rs.
- flux:net `interfaces()`: each interface's `addrs` (an `InterfaceAddr[]`).
  packages/flux-types/modules/net.d.ts; built in
  flux/src/forge_plugins/net.rs. Read by packages/cli/src/server/main.ts
  (the LAN address) and examples/screencast/src/index.tsx.

## Why not now

Each is an app-visible field: renaming it breaks every app that reads it,
with no compile error in plain JS. It is a surface break, to batch with
other renames or to do with a transition (both names for a while).

## Shape

Rename `addrs` to `addresses` (and `ConnAddr.addr` to `address`, the type
names to match) in the three modules, their d.ts, the in-repo readers and
the tests; decide whether the old names stay readable for one release.
