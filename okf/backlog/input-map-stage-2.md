---
title: Input map stage 2 - rebinding and interactions
description: The map (core input-map.ts) covers actions, sources, presets, drive(), by-name injection, enable/disable contexts and gamepad.next() joining, and components' focus navigation consumes it; what a settings screen and a game want next is a rebinding flow over bindings() with a serializable form, and hold/tap/chord interactions on buttons - both waiting for a consumer.
created: 2026-09-07
---

# Input map stage 2

## Symptom

Stage 1 (okf/notes/input-map-design.md) gives every control a map to
drive it and the app a place to bind devices; stage 2 put components'
focus navigation on the map with the UI bindings built in, added
enable/disable contexts by action name, and `gamepad.next()` for split
screen. Two things a settings screen or a game wants are still
hand-written, and stay here until something in the repo needs them:

- **Rebinding.** `bindings()` lists what is bound with display labels and
  `unbind` removes one, but there is no "press the key you want" flow
  that listens for the next input and binds it, nor persistence of a
  map's bindings.
- **Interactions on buttons.** Hold, tap, double-tap and chords (Unity's
  Interactions, Unreal's Triggers) are recognized by each consumer over
  onPress/onRelease timing.

## Done looks like

- A `rebind(action)` that resolves with the next source actuated, plus a
  serializable form of a map's bindings (source labels are the display
  half; a stable id per source is the missing half).
- `hold`, `tap`, `chord` sources or action options, producing button
  actions from other button sources.

## Not in this item

A `back` UI action: Escape and the pad's east button are deliberately not
back triggers, the native back event has its own handler stack (onBack),
and nothing in components consumes one yet. Rumble (okf/backlog/gamepad-haptics.md), MCP-side input hold
(okf/backlog/mcp-input-hold.md), a control-API endpoint that injects
actions by name (the debug commands do it per app today).
