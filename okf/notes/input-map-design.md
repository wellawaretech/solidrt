---
title: Input map design - actions between devices and controls
description: The decisions behind core's input map (createInputMap, createAxes, the devices and the pointer feed), each against Godot's InputMap and Unity's Input System, the shared control vocabulary, and the traps met cutting the camera controls over.
created: 2026-09-07
---

# Input map design

True regardless of the work that produced it (the 2026-09-07 cut-over of
the 2d and 3d camera controls to `@solidrt/core`'s input map).

## The rule

ARCHITECTURE.md: a control consumes a device-free abstraction and never
handles events or reads a device itself; devices are adapters the app
binds. The map is the named, typed layer between the two, and the
components carry no device wiring at all (`<OrbitCamera />` without an
`input` map moves only through its handle). Presets are plain bindings
the app applies and edits, never a default.

## Against the engines

| | Three | Godot InputMap | Unity Input System | Ours |
|---|---|---|---|---|
| Abstraction | none, controls bind DOM | named actions, strengths | typed actions in maps | typed actions per map instance |
| Deltas | n/a | not actions (mouse motion read directly) | mouse delta shares the value with a stick, consumers scale by dt | a second channel per action, device-free units, brackets |
| Combination | n/a | latest strength per action | most-actuated control wins | sum, then clamp (axis to -1..1, vec2 to unit length) |
| Injection | fake DOM events | `action_press`, `parse_input_event` | virtual devices (an on-screen stick pretends to be a pad) | by name: set/press/nudge/begin/end |
| Multiplayer | n/a | global singleton, `move_left_p2` action names | PlayerInput per player plus a join manager | one map per player, bound to `gamepad(slot)` or `gamepad.next()` |
| Pointer scope | element | global | global (`Pointer/delta`) | per element feed: the tree holds UI and scene together |
| Processors | n/a | deadzone per action | invert, scale, deadzone, normalize | invert, scale; dead zone in the pad device |
| Presets | n/a | `ui_*` actions built in | Default Input Actions asset | code the app applies (`orbitBindings`, ...); the UI set built into the focus nav |
| Contexts | n/a | none | action maps enabled per scheme | enable/disable by action name on one map |

## Decisions

- **Two channels per axis action.** A rate (sampled, integrated by the
  consumer at its own speed) and deltas (immediate, normalized). Unity's
  single value forces the deltaTime hack in every controller; Godot's
  omission forces mouse look outside the map.
- **Device-free delta units.** Drag travel in element heights (a drag
  across the element is 1 whatever the window: Three's OrbitControls
  convention, already the controls' internal rule), zoom in octaves,
  twist in turns, mouse motion under lock through a fixed reference
  (1500 px per unit, Three's 0.002 rad/px). The pointer feed normalizes
  by the element's laid-out box, which a detached d-* leaf does not have:
  the feed throws at the first press unless created with `layout`.
- **Brackets tell a finger from an impulse.** A pinch's deltas arrive
  between begin and end; a wheel notch arrives alone. The 2d camera
  applies bracketed zoom exactly and eases unbracketed zoom; the orbit
  holds one anchor per pinch and anchors per notch. Brackets fire on the
  press itself (before the recognizer's slop) so a glide stops the moment
  a finger lands.
- **Screen convention everywhere.** x right, y down, a stick pushed up
  reads y = -1 (the web Gamepad API). `move` means forward by -y. Keys
  and sticks move the CAMERA where a drag moves the content, so presets
  bind them to `pan`/`rotate` through `invert()`; `look` is the exception
  (drag and stick both turn the eye).
- **The vocabulary is one set of words, per-control subsets.** `pan`
  vec2, `zoom` axis, `rotate` vec2 (orbit), `roll` axis (2d), `look`
  vec2, `move` vec2, `rise` axis - one kind and unit per word, so one
  binding block drives the orbit and the 2d camera alike. The 2d
  "rotation" became `roll` so `rotate` keeps one kind.
- **Sum-and-clamp** rather than most-actuated: keys and a stick on one
  action add, and the unit clamp on a vec2 fixes the classic 1.41x
  diagonal walk for every control at once.
- **Map per instance**, not a singleton: split screen is two maps, each
  on a pad slot or on `gamepad.next()`, the device that claims the next
  unclaimed pad to press any button (Unity's join manager) and frees it
  when its scope is disposed. Runtime-free in input-gamepad-device.ts so
  the join order is checked headless; verified with physical pads on
  packages/core/examples/gamepad.tsx (the pad test bed: raw snapshot, two
  joining players, values live, a marker per player).
- **Contexts are a switch per action, not a second map.** `enable` and
  `disable` take action names; a set is a list of names, which a preset's
  action object already is. A disabled action reads neutral, drops its
  deltas and closes the gesture it had open (the map counts delivered
  brackets, so raw onGesture listeners never see an unmatched end); its
  sources keep their state, so a key still held when the action comes
  back reads at once (Unreal keeps, Unity resets). Edge callbacks see the
  switch as a release or a press.
- **The UI layer binds a default; app controls do not.** Focus navigation
  in components consumes `navigate` (vec2), `cycle` (axis) and `select`
  (button) - Flutter's intents, Godot's ui_* actions, Unity's UI module -
  and, created bare, binds `uiBindings` over the keyboard and every pad on
  a map of its own (`nav.input`), because a component library must work
  with nothing wired, as all three do. The cameras stay inert without a
  map. Repeat is the nav's own timing over the rate (Unity's UI module),
  so dpads and sticks walk the way keyboards did through key repeat, and
  activation is one path (`select` into the nav-action registry) where
  createPress used to read Enter and Space itself.
- **Key specs carry modifiers** ("Shift+Tab", "Ctrl+KeyS"). Within one
  source the most specific matching spec wins a down, so
  `axis("Shift+Tab", "Tab")` reads -1 rather than 0; an up releases on the
  bare key, so a modifier let go first cannot leave the key stuck.
- **Pointer gestures take the same chord, on the device, not as a
  processor.** `pointer.drag("Ctrl")` is the variant the feed opens
  instead of the bare `drag` when the event opening the bracket carries
  Ctrl (input-chord.ts is the one grammar for both devices). Three rules
  exist for a button bound bare and chorded: both fire (Godot's
  `is_action_pressed` without `exact_match`, Unity by default), exact
  match where bare means no modifiers (Blender's keymap items), and
  most-specific-wins (Unreal's automatic ChordBlocker, Unity's
  `shortcutKeysConsumeInput`, Three's OrbitControls choosing PAN over
  ROTATE at mousedown). Both-fire is the bug every viewport then fixes by
  hand; exact match kills a bare gesture under any stray modifier and
  contradicts the keyboard rule above (Shift-run must keep WASD walking);
  most-specific-wins changes nothing until a chord is bound. A processor
  (`chord(pointer.drag, "Ctrl")`) cannot give it: a wrapper does not see
  its sibling bindings, and carrying modifier flags through DeltaSink
  would put a pointer+keyboard concept on the contract gamepads and
  by-name injection share. Resolved once per bracket at its opening event
  and held (Three decides at mousedown; Unreal's continuous chord splits
  one gesture across two actions); wheel and mouseDelta per event. Touch
  has no modifiers, so a chord is the desktop path and presets keep the
  bare or two-finger binding for fingers. A bracketed spec may end in
  its button (`drag("Right")`, `drag("Shift+Middle")`), as a key spec
  ends in its key; a button is a discriminator, not a modifier (a right
  drag never feeds a Left spec), and bare means Left, so nothing bound
  before buttons changed. One button per pointer: a second button on a
  held mouse joins nothing and only the held button's release closes the
  gesture. Three's middle-drag dolly is still not a preset binding: zoom
  is an axis and a drag is a vec2, and no processor projects one onto
  the other yet.

## Traps

- Solid 2 defers signal writes until a flush: a list kept only in a
  signal reads stale in the same synchronous block, so the second of two
  `bind` calls clobbered the first. Plain state is the truth (source
  lists, script values, the orbit's auto-orbit switch); signals only
  notify, with `ownedWrite` because bind/add/set run from component
  bodies.
- A control's `active()` gate must stay a memo over reactive sources; a
  rate source over plain variables never wakes the loop (the caller runs
  update(dt) itself then).
- The 2d fling: with immediate pushes on every nudge the camera no longer
  went active during a drag, so the velocity estimate saw the whole drag
  as one frame at release (a 4500 px/s fling). A pan gesture in flight
  keeps the camera active. And a finger held still before lifting must
  not fling: the resampler's last correction lands as a small stray delta
  in the release frame, so the fling gates on the smoothed velocity too.
- Verifying pans with synthetic input: the 2d camera example starts at
  the fit zoom, where the contain clamp makes every pan a no-op (nothing
  to pan) - zoom in first. A synthetic up right after the last move drops
  the last resampled segment (the recognizer resets on up), so a drag
  reads about 20% short of its pixels; give the up 40-80 ms.
- With Space bound to `select`, a typed Space in a text field reached the
  window (the editor consumed only the keys it handles) and submitted the
  field through its own nav action. The editor now consumes every
  printable key while a text session is active: they are its text.
- Headless: `@solidrt/core/input` is the runtime-free entry (map, axes,
  keyboard, processors). The gamepad device and the pointer feed need the
  event bus.
