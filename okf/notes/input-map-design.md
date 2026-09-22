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
| Rebinding | n/a | `action_add_event` from `_input` by hand | `PerformInteractiveRebinding` | `rebind(action, devices)` over each device's `listen` |
| Persistence | n/a | project settings | binding-override JSON | `save()`/`load()` over source ids |
| Interactions | n/a | none | Hold, Tap, MultiTap, Press (Unreal: Triggers) | `hold`, `tap`, `doubleTap`, `chord` as button sources |
| Schemes | n/a | none | control schemes, `PlayerInput.currentControlScheme` | `device()` plus `bindings()` by `source.device` |
| Gestures | none (controls bind DOM) | none (addons) | none (samples) | `swipe("Left")`, `longPress`, `doubleTap` as pulsing button sources; velocity on the drag's `end` |

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
- **Ids name the source, not the device instance.** `gamepad:button:south`
  has no slot, so player 2's saved file restores onto player 2's device;
  the id grammar is `<device>:<spec>` with processors around it
  (`invert(...)`, `hold(400,...)`), the spec being the device's own
  vocabulary, and `load()` resolves through the devices the app hands in
  and throws on anything else, so a file from another version fails
  loudly rather than half-binding. Labels stay the display half.
- **Rebind listens where the input arrives.** The map owns the key
  handlers, so it captures the next key itself (and swallows it: the
  player is naming a key, not playing); a pad and a pointer feed each
  carry `listen(kind, found)`, the pad over its snapshot with what is
  held at the start excluded until released, the feed on the bracket that
  opens (as the chord and button variant that opened it). `replace`
  swaps the action's bindings of the found device, not the others, since
  a settings row reads "Jump: Space" per device; a key on an axis/vec2
  action means one `part` of the bound composite, the way Unity rebinds a
  composite part by part. Found sources apply on a microtask: they arrive
  from inside a device's effect.
- **Interactions are sources, not action options.** `hold(pad.button("west"))`
  binds next to the plain press on another action and the consumer reads
  a bool; timing is `setTimeout` (headless, no frame loop), and a tap is
  a one-task pulse so the map's onPress edge sees it.
- **The active device is the last one that moved.** One effect per
  binding on the source's rate, plus the delta path; a custom source
  without `device` never counts. Glyphs are the app's assets keyed by id;
  core gives the id, the label and the device.
- **Discrete gestures are pulsing buttons on the feed, and the fling is
  a fact on the end bracket.** A swipe, a long-press and a double-tap
  are what a game binds next to a pad button, so they are button sources
  that read pressed for one task (the `tap()` shape), with the swipe's
  direction as the spec's trailing word (the drag's button grammar) and
  the chord taken from the down that opened the gesture. They run on the
  feed's own events rather than as arena recognizers of their own: the
  feed is one claimant (its transform), and a second owner resolving the
  arena would refuse the transform's later steal - drag-after-hold on a
  viewport would die. The release velocity rides `end(velocity)` in the
  source's units per second (element heights for a drag), through
  invert/scale (negated, scaled), `drive()` and `input.end(action, v)`;
  the cameras fling from it and keep no estimator of their own. A
  button press carries no position, so double-tap-to-zoom is not a
  preset binding: an app puts `createDoubleTap` on the view and calls
  `zoomAt` with the tap's point.

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
- The 2d fling used to estimate its own release velocity from the drag
  deltas per frame (an EMA that needed the camera active during the drag
  and a gate against the resampler's stray last delta). The gesture
  measures it now (velocity.ts under every recognizer) and the camera
  takes it from `end(velocity)`; see pointer-coalescing-traps.md for the
  velocity-at-lift trap.
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
