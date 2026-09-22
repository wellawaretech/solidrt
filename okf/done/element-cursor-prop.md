---
title: Mouse cursor - element cursor prop, hidden cursor, image cursors
description: The web's cursor model through the SolidRT lens - a per-element cursor prop resolved against the hovered path in the router (innermost wins, "none" hides), the platform's 20 shapes by CSS name, and image cursors registered once via createCursor with HiDPI alternates and animation through SDL 3.4.
created: 2026-09-02
---

# Mouse cursor - element cursor prop, hidden cursor, image cursors

## Symptom

An app could not change the mouse cursor at all: no pointer-hand over a
pressable, no I-beam over text input, no resize arrows on a
drag-to-resize edge, no way to hide the arrow for an in-tree cursor.
`AlloyCommand::SetCursor` carried a 12-shape enum and `SetCursorVisible`
sat beside it, but nothing sent either, and the loop handler set the
cursor through a wrapper whose Drop destroyed it on the spot (SDL reverts
to the default when the current cursor is destroyed), so it would have
flashed and reverted had anything sent it.

## What was built

- **`cursor` prop on every element** (`PointerProps.cursor`): a CSS name
  (`default`, `pointer`, `text`, `wait`, `progress`, `crosshair`, `move`,
  `not-allowed`, the four axis resizes and the eight directional resizes:
  the 20 shapes SDL provides), `"none"` to hide the cursor, or a
  `createCursor` handle for an image. The innermost hovered element that
  sets one wins; none on the path means the default arrow, sent
  explicitly on the transition so the window recovers from whatever the
  last hovered element set.
- **Resolution lives in the router**, not JS. `PointerRouter` already
  owns the hovered path per pointer, diffs it on every move and re-runs
  it after every frame (layout shifts under a stationary pointer), and
  enter/leave deliveries are gated by interest bits, so JS never sees the
  whole path. `HitConfig.cursor` holds the per-node value; `update_hover`
  resolves the mouse path's innermost cursor on every hover update (also
  when the path is unchanged: a hovered node can change its cursor in
  place); `take_cursor_change` reports a change exactly once, so nothing
  is sent per move. flux's input plugin sends `SetCursor` over the
  command channel after each dispatch and each frame refresh.
- **The loop owns a cursor cache** (`alloy/src/cursor.rs`): system shapes
  created on first use, image cursors on `CreateCursor`, each an
  `OwnedCursor` (sdl_utils) destroyed with the cache. All 20 shapes go
  through `SDL_CreateSystemCursor` directly; the sdl3 crate's enum stops
  at 12. `Hidden` is SDL's hide, undone by the next non-hidden apply.
- **Image cursors** register once (`createCursor` in core: encoded bytes,
  decoded to straight alpha, hotspot, optional HiDPI alternates keyed by
  scale, or frames with durations for an animated cursor) and are
  referenced by handle. The loop builds the base surface, attaches the
  alternates with `SDL_AddSurfaceAlternateImage`, and creates a color or
  animated cursor; SDL copies the pixels during the call. The DPI-scale
  hint is set with the other startup hints. Handles are dropped with the
  owning reactive scope, and the flux tree state drops every live handle
  with the engine, so a reload leaves nothing in the loop's cache.

## Decisions

- **OS cursor, not an in-tree sprite.** A drawn cursor lags a frame,
  freezes during a stall and clips at the window edge. The hidden cursor
  plus an ambient `onPointerMove` follower stays the route for reactive
  cursors (direction arrows, trails); a direction arrow also works OS-side
  as a handful of pre-rotated handles switched on direction change.
- **Handle per element, not re-skinning a shape slot.** The web and Unity
  put a custom image on an element; Godot and Unreal replace a shape
  globally. A window-level shape-to-handle override is a small addition
  if a game wants to theme every arrow; not part of this.
- **Names SDL has no shape for are rejected** (grab, zoom-in, ...): an
  app supplies those as image cursors rather than getting a silent
  nearest-shape substitute.
- **Mouse and pen.** A hovering pen has an OS cursor; touch has none.
- **Pointer lock needs no special handling**: SDL's redraw hides the
  cursor while relative mode is on whatever is set, positions freeze so
  the hovered path does not change, and the set cursor reappears on
  unlock.

## Compared elsewhere

Per-element resolution with innermost-wins is what Godot (`Control.mouse_default_cursor_shape`),
Unreal (widget `Cursor`), Flutter (`MouseRegion.cursor`) and the web do.
None of Godot, Unity or Flutter has multi-resolution cursors, and only
Unreal on Windows and some browsers animate; SDL 3.4 gives both in one
call. Everyone caps the size: the web at 128px, Godot at 256px, Unity
recommends 32px; the core docs state 128px as the ceiling.

## Component defaults

Every pressable component root (Pressable, Button, Checkbox, Switch,
Radio, Select and its options, SegmentedControl segments, NavShell
items, ContextMenu items, and an Item with `onPress`) shows the pointer
hand; Pressable's own `cursor` prop overrides it. A passive Item shows
nothing, as it attaches no press recognizer. The editor field under
TextInput and RichTextEditor shows the I-beam. Disabled controls take no
pointer events, so the enclosing cursor shows there.

## Known limits

X11 and Android show the 1x image only; the DPI hint scales on Windows,
macOS and Wayland pick the alternate.
