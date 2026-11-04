---
title: Move CI to the macos-15 runner
description: On the macos-15 GitHub runner the flux:mdns browse and services tests fail with "No route to host (os error 65)" on the multicast send, most likely macOS 15 Local Network privacy, so CI and release stay on macos-14 until the darwin mDNS tests are handled.
created: 2026-11-04
---

# Move CI to the macos-15 runner

CI and the release workflow run on `macos-14` (three runners in
`.github/workflows/ci.yml`, the runner and its rust-cache key in
`release.yml`). A move to `macos-15` (CI run 41872692685, commit 91b08507)
failed `test-js (darwin)` in `flux/tests/mdns.test.ts`:

```
FAILED: browse of an unknown service resolves to an array
  Error: No route to host (os error 65)
FAILED: services resolves to an array
  Error: No route to host (os error 65)
```

The same tree passed all three mDNS tests on `macos-14` the same day (the
dependabot cargo-minor-patch run 41852532866), so the runner image is the
cause, not a dependency. `resolve([])` passes because it never touches the
network. The other darwin jobs had not failed when the move was rolled back.

## Cause

`forge::mdns::query` (forge/src/mdns.rs) binds 5353, joins the group and
sends the question to 224.0.0.251. The send fails with `EHOSTUNREACH` and
`query` returns the error, so `browse` and `services` reject. On macOS 15 a
process without Local Network permission gets exactly that error on a
multicast send, and a runner cannot grant the permission from a script.
This is the likely cause; it is not confirmed on a Mac.

Rejecting is the correct behavior: an app whose user denied Local Network
access must see the error, not an empty list that reads as "no services on
the LAN". Do not make `query` swallow the send error.

## Done looks like

- All five `macos-14` references move to `macos-15`.
- The two mDNS tests do not fail the darwin job. The cheapest way is a
  `platform !== "darwin"` guard from `flux:process`, the pattern the `win32`
  guards in flux/tests/path.test.ts and process.test.ts use, with a comment
  that names the cause. That also skips them on a dev Mac where they would
  pass; a guard that skips only on `EHOSTUNREACH` keeps them there.
- First confirm on a Mac that a denied Local Network permission gives
  `EHOSTUNREACH` on the send, so the guard comment states a fact.
- Check the rest of the darwin jobs on `macos-15` (test-forge, test-flux and
  the release build) before the move lands.

## Related

`sol pack` makes no macOS `.app` bundle today (a desktop pack is the runner
binary with a trailer), so no Info.plist exists to carry the Local Network
keys. When macOS bundling arrives, an app that uses `flux:mdns` needs
`NSLocalNetworkUsageDescription`, and `NSBonjourServices` for the service
types it browses. [mdns-discovery.md](mdns-discovery.md) is the other open
mDNS item.
