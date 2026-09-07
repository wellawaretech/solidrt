---
title: Synthetic gamepads through the control API
description: An agent cannot verify anything a pad drives (the input map's gamepad device, gamepad.next() joining, the camera presets' stick and trigger bindings, focus navigation on the dpad) because /input and send_input know pointer, key, wheel and text events only; the client should accept synthetic pads (connect, buttons, axes, disconnect) that enter where SDL's do, so a pad session is scriptable and headless-verifiable like a drag or a keystroke.
created: 2026-09-07
---

# Synthetic gamepads through the control API

## Symptom

Every pad-driven path landed in 2026-09-07's input map work is verified
by typecheck, review and the human with two pads in hand: the gamepad
device's sources, `gamepad.next()` seating players in pick-up order, the
orbit, first-person and 2d camera presets on sticks and triggers, the
focus navigation's dpad, left stick and south button. The core check
(`packages/core/checks/input-gamepad-check.ts`) drives the device over a
signal of snapshots, which proves the JS side but not the pipeline: the
runtime's coalescing into one sticky `gamepads` event per loop
iteration, the slot assignment on connect and disconnect, the dead zone
against real axis values, a held button surviving a loop drain. The
`gamepad.tsx` test bed exists for the human; the agent has nothing.

## Why

`POST /input` (`send_input`) parses pointer, key, wheel and text events
(`lattice/src/go/connection.rs`, tests in `lattice/src/tests/input.rs`)
and injects them where SDL's arrive. Pads have no such entry: their
state lives in `alloy/src/gamepad.rs` (`Gamepads`, slots filled from
SDL's connect and disconnect, `snapshot_event` per loop iteration) and
reaches JS as the sticky `gamepads` event
(`flux/src/alloy_plugins/events.rs`), read through core's `gamepads()`.

## Done looks like

- `/input` events of type `gamepad`: `{ type: "gamepad", action:
  "connect", slot?, name? }` seats a synthetic pad (the lowest free slot,
  as SDL's do), `{ action: "set", slot, buttons?: [...], axes?: {...} }`
  holds the named buttons and axis values until the next set (SDL names:
  "south", "dpadUp", "leftX", "rightTrigger", ...), `{ action:
  "disconnect", slot }` frees the slot. `holdMs` and `delayMs` as on the
  other events, so a tap of south or a stick pushed for a second is one
  event.
- Synthetic pads live in `Gamepads` next to the real ones, so a snapshot
  interleaves both and everything downstream (coalescing, the sticky
  replay on subscribe, `take_back_edge` for the back button, the dead
  zone in the device) sees no difference. Synthetic state is level, not
  edge: a set holds until the next set, as a physical pad's does.
- The `send_input` tool schema gains the event; the debugging guide
  (`packages/cli/agents/debugging.md`) says a pad is driven like a key.
- A driven run on `packages/core/examples/gamepad.tsx`: connect two pads,
  press south on the second, then the first, and read from the tree that
  player 1 is the second pad's slot and player 2 the first's; push a
  stick and read the marker moving from a snapshot; disconnect one and
  read the slot going empty.

## Not in this item

Recording physical pad sessions to a script (`srt run --capture` takes
keys only; pointer and pads are one later step together), rumble
(okf/backlog/gamepad-haptics.md), and holding physical input while
driving (okf/backlog/mcp-input-hold.md).
