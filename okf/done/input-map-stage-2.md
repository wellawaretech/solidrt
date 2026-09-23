---
title: Input map stage 2 - rebinding, interactions, active device
description: The map covered actions, sources, presets, drive(), by-name injection, contexts and gamepad.next() joining; a settings screen and a game still hand-wrote rebinding, saved bindings, hold/tap/chord recognition and button prompts per device. Stable source ids, save()/load(), rebind() over the devices, hold/tap/doubleTap/chord as button sources and a reactive device() close that.
created: 2026-09-07
completed: 2026-09-22
---

# Input map stage 2

## Symptom

Stage 1 (okf/notes/input-map-design.md) gives every control a map to
drive it and the app a place to bind devices; stage 2 put components'
focus navigation on the map with the UI bindings built in, added
enable/disable contexts by action name, and `gamepad.next()` for split
screen. Two things a settings screen or a game wants were still
hand-written:

- **Rebinding.** `bindings()` lists what is bound with display labels and
  `unbind` removes one, but there is no "press the key you want" flow
  that listens for the next input and binds it, nor persistence of a
  map's bindings.
- **Interactions on buttons.** Hold, tap, double-tap and chords (Unity's
  Interactions, Unreal's Triggers) are recognized by each consumer over
  onPress/onRelease timing.

And a third, from the engine comparison: nothing tells the app which
device the player is on, so button prompts (keyboard glyphs against pad
glyphs) cannot follow them - Unity's control schemes.

## What was done

- **Ids.** Every source carries `id`, a serializable spec without the
  device instance (`keyboard:key:Shift+Tab`, `gamepad:button:south`,
  `pointer:drag:Ctrl+Right`, processors around it:
  `invert(gamepad:leftStick)`, `hold(400,keyboard:key:Space)`), and
  `device` (`keyboard` | `gamepad` | `pointer`). The grammar and
  `resolveSource(id, devices)` live in core `input-id.ts`; each device
  has `resolve(spec)` for its half. `input.save()` returns ids per action,
  `input.load(saved, devices)` resolves and rebinds, all-or-nothing.
- **rebind(action, devices, { signal, replace, part })** returns a promise
  of the source bound: the next key down through the map's own handlers
  (captured, not played), the next pad button, trigger, raw axis or stick
  past a threshold (`GamepadDevice.listen`, what is held at the start
  must be released first), the next pointer gesture's opening bracket as
  the chord and button variant that opened it (`PointerFeed.listen`).
  `replace` (default) swaps the action's bindings of the found source's
  device; `part` swaps one key of the bound keyboard composite on an
  axis/vec2 action; `signal` is the web AbortSignal (flux's subset:
  `onabort`, chained and restored).
- **Interactions** in `input-processors.ts` (invert and scale moved
  there): `hold(source, ms)`, `tap(source, ms)`, `doubleTap(source,
  gapMs, tapMs)`, `chord(...sources)`, button sources from button sources.
  A tap reads pressed for one task (`setTimeout 0`), enough for the map's
  onPress edge, then releases on its own; timing is `setTimeout` under a
  root disposed with the creating scope.
- **device()**: reactive, the device that last moved anything bound
  (one effect per binding on the source's rate, plus the delta path).
  Button prompts are `bindings(action)` filtered by `source.device`; the
  glyph images are the app's assets, keyed by `id`.
- Checks: `checks/input-map-check.ts` (ids, save/load round trip and its
  failure atomicity, rebind over keys with modifiers, parts and abort,
  rebind over listening devices, the four interactions with real
  timers, device()); `checks/input-gamepad-check.ts` (ids, resolve,
  listen with the held-at-start rule).

Two checks in the contexts section of input-map-check.ts ("a source
still held presses on enable", "enabling an enabled action is a no-op")
fail at HEAD before this work with the current flux binary; left as
found, noted in okf/tiny.md.

## Not in this item

A `back` UI action: Escape and the pad's east button are deliberately not
back triggers, the native back event has its own handler stack (onBack),
and nothing in components consumes one yet. Rumble
(okf/backlog/gamepad-haptics.md), MCP-side input hold
(okf/done/mcp-input-hold.md), a control-API endpoint that injects
actions by name (the debug commands do it per app today), on-screen
touch controls (a virtual stick is a device with `listen`/`resolve` of
its own, nothing in the repo asks for one yet).
