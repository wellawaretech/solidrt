---
title: Pointer gestures cannot carry a modifier chord
description: Ctrl-drag pans while plain drag orbits is the default of every 3d viewport, and it cannot be a binding - createPointerFeed reads the whole PointerEvent on the down that opens a gesture and forwards only the pointer id, so an app has to reconstruct the modifier state one call later through a phantom keyboard action or a handler-ordering trick.
created: 2026-09-08
completed: 2026-09-08
---

# Pointer gestures cannot carry a modifier chord

## Symptom

The binding every 3d viewport wants - **plain left-drag orbits, Ctrl +
left-drag pans**, the default in Blender, Maya, most CAD, Figma and
Three's own `OrbitControls` - cannot be written. `orbitBindings`
(packages/3d/src/input.ts) shows the hole: `pan` is bound to
`pointer.pan`, two fingers, and nothing else, so a desktop mouse has no
pan at all.

The fact is not missing, it is discarded. `createPointerFeed`'s
`onPointerDown` receives the whole `PointerEvent` - `ctrlKey`,
`shiftKey`, `altKey`, `metaKey` are all on it, filled from SDL's
modifier state on every pointer event
(flux/src/alloy_plugins/input.rs) - and then calls
`landed(e.pointerId)`, which takes the id and nothing else
(packages/core/src/input-pointer.ts). The six sources are fixed
singletons (`drag`, `pan`, `pinch`, `twist`, `wheel`, `mouseDelta`)
with nowhere to put a chord, and `createTransform` has no modifier
concept either, so by the time a bracket reaches a sink the chord is
gone. Impact is low - nothing is broken - but every viewport app pays
it.

## Why an app cannot add it cleanly

Both routes back are reconstructions of a fact core held one call
earlier:

- **The keyboard device.** `keyboard.key("Control")` read through a
  `button` action (the runtime does report `key: "Control"`,
  `code: "ControlLeft"`/`"ControlRight"` - alloy/src/keymap.rs). Two
  papercuts: a source receives key events **only if bound to an
  action** (input-map.ts `forwardKey` walks the bound sources), so the
  action exists purely to keep the source fed and appears in
  `bindings()` as if it were a control the user could rebind; and the
  held count is signal-backed, so it cannot be read back in the tick
  that set it.
- **Wrapping the leaf's handlers ahead of the feed** to latch
  `e.ctrlKey` before `begin()`. Works only because `feedPointer`
  registers its root listener before the leaf's own prop listeners -
  that ordering is real but incidental, the app does not control it and
  no doc promises it.

Either way the gate itself becomes app code: about fifteen lines with
three ways to get it wrong, none of which fails loudly. Decide per delta
instead of per bracket and a modifier tapped mid-drag splits one gesture
across two actions; forget to suppress `end` when `begin` never fired
and the consumer closes a gesture it never opened; latch the predicate
at bind time and it never updates.

## Not a third processor

`invert` and `scale` are pure value transforms with no dependency
outside the source. A gate takes a reactive predicate, which widens what
a processor means, and it still cannot express precedence between two
bindings. The modifier belongs on the feed, where it is just another
property of the gesture and where the opening event is still in hand.

## Done looks like

```ts
pointer.drag            // any modifier - today's behaviour, unchanged
pointer.drag("Ctrl")    // chord, read off the down that opens the gesture
```

- **Same grammar as the keyboard**, which already spells chords
  (`keyboard.key("Shift+Tab")`, modifiers ahead of the key, Shift /
  Ctrl / Alt / Meta). Two devices spelling chords differently is the
  inconsistency, not the feature.
- **Resolved in the feed, once per bracket**, at the event that opens it
  (the down that starts a drag, the second down that starts
  pan/pinch/twist, the up that hands the gesture back to `drag`), and
  held for that bracket. No focus dependency, no microtask lag, no
  ordering trick.
- **Most specific bound spec wins**, exactly as the keyboard device
  resolves `axis("Shift+Tab", "Tab")`: with both `drag` and
  `drag("Ctrl")` bound, a Ctrl-drag feeds only the chord; a bare spec
  keeps ignoring modifiers.
- **Unbracketed sources resolve per event** (`wheel`, `mouseDelta`):
  Ctrl+wheel as a separate action is the same ask one gesture over.
- **Touch has no modifiers**, so a chord binding never fires there. A
  preset that adds a Ctrl-drag pan keeps the two-finger pan as the touch
  path, and that stays true of any preset that grows a chord.

## What was done

`pointer.drag("Ctrl")` and the same call on every gesture, resolved in
the feed per bracket with the keyboard's grammar (input-chord.ts, shared
by both devices); `orbitBindings` binds `pan` to `drag("Ctrl")` beside
the two-finger pan. The decision record is in
[input-map-design](../notes/input-map-design.md). Verified through the
real input pipeline on probes/pointer-chord-probe.tsx: a plain drag moves
azimuth and leaves the target, a Ctrl-drag moves the target and leaves
azimuth to the last digit, and Ctrl pressed after the down keeps the
gesture on rotate.

Button-qualified drags followed the same day:
[pointer-gesture-buttons](pointer-gesture-buttons.md).
