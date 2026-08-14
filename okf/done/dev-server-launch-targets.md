---
title: Dev-server launch targets
description: Start clients from the running dev session with repl targets and launch commands (desktop spawn or adb install), then control endpoints and MCP tools for agents.
created: 2026-07-21
completed: 2026-07-21
---

# Dev-server launch targets

Start clients *from the running dev session* instead of only at CLI
startup: `launch <target>` brings up a client on a named target (local
desktop, a connected Android device, later remote machines), which then
receives the latched push like any connecting client. Complements
`okf/plans/go-client-launcher.md`: server-launched clients always get
the address at launch, shrinking the cases that need the launcher's
QR/discovery/manual-entry pairing to devices the dev machine cannot
reach. Neither plan depends on the other.

## Status quo (2026-07-21)

- Launching is a CLI-startup decision only:
  - `srt run` = server + local desktop client (`spawnClient()` in
    `packages/cli/src/dev-client.ts`, `--dev-server` loopback).
  - `srt client --android` = install + launch on an adb device
    (`spawnAndroidClient()` in `packages/cli/src/dev-android.ts`):
    device/ABI resolution with `--device` prefix disambiguation, APK
    from platform packages, dev-server address as a launch-intent extra
    (emulator via `10.0.2.2`, wireless adb via subnet matching).
  - `srt client --server <addr>` = standalone desktop client.
- The `srt run`/`srt server` foreground process owns the repl
  (`packages/cli/src/repl.ts`: load, stop, reload, list, stats, watch);
  the server runs as a separate spawned process; repl and MCP (`srt
  mcp`) both talk to it via the `/__control__/` API
  (`packages/cli/server/control.ts`).
- Gap: from a running session there is no way to bring up another
  client (e.g. add a phone mid-session without restarting), and agents
  (MCP) cannot launch clients at all.

## Decisions

- The CLI process owns process spawning. Artifact knowledge (client
  binary resolution, APK-per-ABI packages, adb) lives in the CLI today;
  moving spawning into the server (a flux script, which *could* shell
  out via `flux:subprocess`) would duplicate all of it there for no
  gain. Dogfooding deferred, not rejected.
- Repl verb is `launch <target>`, listing verb is `targets`. `launch`
  reads well next to `run` ("run" = start the dev session; "launch" =
  bring a client up on a target). `srt client --android` stays as the
  startup-time equivalent.
- Targets are names, resolved at launch time:
  - `local`: the desktop client on this machine.
  - adb serials (prefix-matched, like `--device` today), shown by
    `targets` with their ABI.
  - Future: configured remote targets (ssh command templates), not in
    scope now.

## Stages

### Stage 1: repl commands

- `targets`: `local` plus connected adb devices with ABI (reuse
  `listDevices`/`printDeviceStatus`; adb absent = local only, no error).
- `launch <target>`: `local` -> `spawnClient()`; serial prefix ->
  `spawnAndroidClient()` against that device (refactor its
  `resolveTarget` to accept an explicit target instead of only
  `--device`/sole-device).
- Multiplicity: today `state.child` tracks one local client and its
  exit drives shutdown bookkeeping; launching a second local client
  needs that to become a list (or `launch local` refuses while one is
  running - decide at implementation, refuse is the minimal first cut).

### Stage 2: control API + MCP

- `/__control__/targets` and `/__control__/launch` on the server, so
  agents can enumerate and launch (e.g. bring up a client to screenshot
  against, mid-session).
- Since the CLI owns spawning, the server must forward launch requests
  to it: the CLI process subscribes over a control WS and executes
  them. This is the one new piece of plumbing; the decision above
  (CLI owns spawning) is what forces it, and it only exists once
  stage 2 starts.
- MCP tools mirror the endpoints (`list_targets`, `launch_client`),
  with `readOnlyHint` on the listing only.

## Futures (out of scope, recorded)

- Configured remote targets (the Windows box over ssh); needs a
  per-target command template story.
- `launch android` booting the emulator AVD when no device is attached.
- Server-owned spawning via `flux:subprocess` if artifact resolution
  ever moves server-side.
