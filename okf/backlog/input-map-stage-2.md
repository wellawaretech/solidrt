---
title: Input map stage 2 - action sets, rebinding, interactions, UI actions
description: The map (core input-map.ts) covers actions, sources, presets, drive() and by-name injection; what a full game or a settings screen wants next is enabling and disabling whole action sets by context, a rebinding flow over bindings(), hold/tap/chord interactions on buttons, a "press to join" pad source for split screen, and the UI's own actions (navigate, select, back) replacing the direct key/dpad reads in components' focus navigation and Button.
created: 2026-09-07
---

# Input map stage 2

## Symptom

Stage 1 (okf/notes/input-map-design.md) gives every control a map to
drive it and the app a place to bind devices. Four things a game or an
app with a settings screen wants next are still hand-written:

- **Contexts.** A menu open, a text field focused, a cutscene running:
  today every consumer checks for itself. Godot has none either; Unity
  has action maps enabled per scheme, Unreal stacks mapping contexts by
  priority.
- **Rebinding.** `bindings()` lists what is bound with display labels and
  `unbind` removes one, but there is no "press the key you want" flow
  that listens for the next input and binds it, nor persistence of a
  map's bindings.
- **Interactions on buttons.** Hold, tap, double-tap and chords (Unity's
  Interactions, Unreal's Triggers) are recognized by each consumer over
  onPress/onRelease timing.
- **UI actions.** components' focus navigation reads arrow keys and dpad
  edges itself, Button reads Enter, Space and the south button: the same
  direct device bindings the cameras just lost, in a package that ships
  presets for nothing.
- **Pad assignment.** A map binds to a slot; a couch game wants "the pad
  that presses south joins as the next player" (Unity's join manager).

## Done looks like

- `input.enable(set)` / `disable` over named action sets, or a second map
  layered with priority - decide against Unity's per-map enable and
  Unreal's stacked contexts; additive on the current shape.
- A `rebind(action)` that resolves with the next source actuated, plus a
  serializable form of a map's bindings (source labels are the display
  half; a stable id per source is the missing half).
- `hold`, `tap`, `chord` sources or action options, producing button
  actions from other button sources.
- `navigate` (vec2), `select` and `back` (buttons) as a components
  preset, with focus-nav.ts and press.ts consuming a map instead of key
  events and gamepads() - the launcher's remote then binds once.
- `gamepad.next()` (or a join source) that claims the next pad that
  presses a button, for split screen without fixed slots.

## Not in this item

Rumble (okf/backlog/gamepad-haptics.md), MCP-side input hold
(okf/backlog/mcp-input-hold.md), a control-API endpoint that injects
actions by name (the debug commands do it per app today).
