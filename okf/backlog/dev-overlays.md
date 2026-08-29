---
title: Client badge for dev-server control and input mute
description: A device connected to a dev server is controlled by it, and a muted client ignores the person holding it; neither is visible on the client today. One always-on line in the existing overlay says CONN or MUTED plus FPS; the stats HUD unfolds under it when toggled.
created: 2026-08-29
---

# Client badge for dev-server control and input mute

## Symptom

A client connected to a dev server looks like any other client, yet the
server owns it: it can be reloaded, driven through `/input`, snapshotted,
and have the person's input muted. Muted (`/__control__/mute`, the repl
`mute`, `srt mcp` mute_user_input) is the sharper case: taps and keys
silently do nothing and the person at the device cannot tell why. See
[mcp-input-hold](mcp-input-hold.md) for the driving-session side.

## Done looks like

One overlay, one layer, one line at the top-right of the safe area,
present whenever the client is connected to a dev server:

```
CONN 60 FPS
```

`MUTED` replaces `CONN` while input is muted (a mute implies a
connection: it clears on disconnect). FPS is last. No server address,
no colours, nothing else always-on. With stats toggled on, the badge
becomes the prefix of the HUD's first line and the HUD's remaining lines
follow, unchanged.

A production `solidrt` build has neither flag and draws nothing, as now.

A user exit (`exit()`, the back chord) from an app while connected drops
the connection and forgets the launch address, so the launcher it returns
to sits idle. Otherwise the server still owned the device and its next
save re-pushed the app the person had just left; and a client started
with `--dev-server` re-dials its launch address on every launcher mount,
which would have taken the latched push straight back into the app. The
server-initiated `stop` (server shutting down) is unchanged: it returns to
the launcher with the connection state it has.

## What it involves

Purely client-side: both facts already live in the client's `DevFlags`
(`connected`, `user_input_muted`), so no wire message, server, console or
MCP change.

- `alloy::StatsOverlay` -> `alloy::Overlay` (`set_overlay`): the retained
  raster-side layer is no longer stats-only. No slot system: one
  declaration, one composite quad per frame.
- `lattice/src/overlay.rs` builds the line from the snapshot's fps plus a
  badge, and the HUD lines below it only when stats is on.
- The draw loop treats `stats_on || badge` as "overlay on"; the badge is
  part of the overlay key so an edge (connect, disconnect, mute on/off)
  rebuilds it, and the connection latches `frame_requested` on those
  edges so the change is drawn on an idle app, as the stats toggle does.
- The once-per-second refresh now runs whenever the overlay is on (FPS is
  on the line): one small re-raster and one forced frame per second on
  connected clients only.

## Found on the way

The badge exposed a stale flag: `DevFlags::connected` (and the mute) were
cleared after `try_serve`'s session loop, but a dev command (`DevCmd::Stop`
from the launcher's Disconnect, or now a user exit) cancels that future
from the supervisor's `select!`, so the teardown never ran and the client
stayed "connected" for log forwarding and the badge while the launcher
said "Not connected". Fixed with a drop guard created at connect.

## Deliberately not done

- Per-badge placement (a centered MUTED pill) would need per-slot layers
  in alloy. Additive later: `set_overlay(slot, ..)` and a per-slot table
  in the draw loop, builders untouched.
- An "identify" flash (light up every client of one server). `CONN` on
  every connected client answers it while one server is up; a per-client
  variant can come back with [mcp-multi-client-ergonomics](mcp-multi-client-ergonomics.md).
