---
title: CI and release run on the macos-15 runner
description: CI and the release workflow moved from macos-14 to macos-15; the flux:mdns browse and services tests, which fail there with "No route to host (os error 65)" on the multicast send (most likely macOS Local Network privacy), are registered off darwin.
created: 2026-11-04
completed: 2026-11-06
---

# CI and release run on the macos-15 runner

The three darwin runners in `.github/workflows/ci.yml` and the darwin
runner and its rust-cache key in `release.yml` moved from `macos-14` to
`macos-15`. CI run #453 passed every job on it, the darwin ones included.

## The mDNS failure

A first move (CI run 41872692685, commit 91b08507) failed
`test-js (darwin)` in `flux/tests/mdns.test.ts` and was rolled back
(84132f08):

```
FAILED: browse of an unknown service resolves to an array
  Error: No route to host (os error 65)
FAILED: services resolves to an array
  Error: No route to host (os error 65)
```

The same tree passed on `macos-14` the same day, so the runner image is
the cause. `forge::mdns::query` (forge/src/mdns.rs) binds 5353, joins the
group and sends to 224.0.0.251; the send fails with `EHOSTUNREACH`. On
macOS 15 and later a process without Local Network permission gets that
error on a multicast send, and a runner cannot grant the permission from a
script.

The cause is likely, not confirmed. On a Mac (macOS 27.2) both queries
pass from an ssh shell, which Local Network privacy does not cover, so
that does not show the denied case. A launchd job would, but the test
account there has no GUI session and loading a user agent needs root.

Rejecting is the correct behavior: an app whose user denied Local Network
access must see the error, not an empty list that reads as "no services on
the LAN". `query` does not swallow the send error.

## Resolution

The two network tests are registered inside `platform !== "darwin"` (from
`flux:process`), the pattern of the `win32` guards in path.test.ts. This
also skips them on a dev Mac where they would pass; a guard on
`EHOSTUNREACH` only was rejected as fussier. `resolve([])` never touches
the network and still runs everywhere.

CI does not build `make dist PROFILE=release-opt`, so the darwin release
build first runs on `macos-15` at the next release.

## Related

`sol pack` makes no macOS `.app` bundle today, so no Info.plist exists to
carry the Local Network keys. When macOS bundling arrives, an app that
uses `flux:mdns` needs `NSLocalNetworkUsageDescription`, and
`NSBonjourServices` for the service types it browses.
[mdns-discovery.md](../backlog/mdns-discovery.md) is the other open mDNS
item.
