---
title: Hold physical input while the MCP bridge drives the app
description: A person touching the keyboard or mouse while an agent verifies through send_input corrupts the run (focus moves, text lands in the field under test, snapshots show mixed state); the client should be able to ignore physical input for the duration of a driven session, visibly, and hand it back on request or timeout.
created: 2026-08-19
---

# Hold physical input while the MCP bridge drives the app

## Symptom

An agent drives the local client through `mcp__solidrt__send_input`,
`call_debug`, `get_snapshot`, while the client is a real window on the
developer's desktop. Any keystroke or click the developer makes in the
meantime (often by accident: the window has focus, they type into their
editor, the client swallows it) mixes into the driven session: text lands
in the field under test, focus jumps, a snapshot shows a state nobody
asked for. Both sides then distrust the result and redo the run.

## Why

Physical input and control-API input arrive on the same path (SDL events
and `/input` events both end up in the frame's input batch); nothing
distinguishes "the machine is testing" from "the person is using". The
client has no notion of being driven.

## Done looks like

- A control-API knob, `POST /input-hold {"held": true|false, "timeout": ms}`
  (MCP tool `hold_input`, and `send_input` may take `hold: true` for the
  common one-shot case): while held, the client drops SDL keyboard, mouse,
  touch and gamepad events (window/lifecycle events still flow) and only
  `/input` events reach the app.
- Visible: the stats overlay or a corner badge says "input held" so the
  developer knows why the window ignores them.
- Self-releasing: a timeout (default a minute, refreshed by every driven
  call) and release when the bridge disconnects, so a crashed agent never
  leaves a deaf window. Escape hatch: a keyboard chord (say Ctrl+Shift+Esc)
  always releases.
- Policy lives in lattice's dev/control layer; alloy delivers events as
  today (fact source, no test-mode special case).
