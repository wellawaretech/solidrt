---
title: Wire up the mouse cursor - element cursor prop over SetCursor
description: AlloyCommand::SetCursor and SetCursorVisible exist with a CSS-vocabulary Cursor enum but have no sender anywhere; give apps the web's cursor model - a per-element cursor prop resolved against the hover path, innermost wins, "none" hides.
created: 2026-09-02
---

# Wire up the mouse cursor - element cursor prop over SetCursor

## Symptom

An app cannot change the mouse cursor at all: no pointer-hand over a
pressable, no I-beam over text input, no resize arrows on a
drag-to-resize edge. The platform side is ready and idle -
`AlloyCommand::SetCursor(alloy::Cursor)` carries the 12-shape CSS
cursor vocabulary and the run loop applies it via SDL, and
`SetCursorVisible(bool)` sits beside it - but nothing sends either
command: no flux binding, no core surface, no JSX prop
(the alloy enum landed with the architecture-review item-7 boundary
work; the wiring was out of scope there).

## What done looks like

The web model, through the SolidRT lens: a `cursor` prop on elements
(`<rect cursor="pointer">`), the CSS names the alloy enum already
mirrors (`default`, `text`, `wait`, `crosshair`, `progress`,
`nwse-resize`, `nesw-resize`, `ew-resize`, `ns-resize`, `move`,
`not-allowed`, `pointer`) plus `none` (maps to
`SetCursorVisible(false)`, back to visible on leaving it). While the
pointer hovers a node, the innermost element on the hover path with a
`cursor` prop wins; no prop anywhere means the default cursor. Built-in
components pick sensible defaults later (TextInput sets `text`,
Pressable sets `pointer`) - not part of this item.

## Shape

Cursor is interaction state, not paint: it can resolve entirely
JS-side against the hover path core already maintains for
enter/leave dispatch (`path_diff`), the same way event handlers live
JS-side without a rendertree field. The rendertree does not change.

- flux gui: a `setCursor(name: string)` / `setCursorVisible(visible)`
  binding on the tree store, cloning `alloy_cmd_tx` exactly like
  `set_pointer_lock` (alloy_plugins/tree.rs:459); parse the CSS name to
  `alloy::Cursor` in the plugin (throw in dev on an unknown name, per
  the validation policy). flux-types gui .d.ts in the same change.
- core: keep `cursor` in the JS-side per-element interaction
  bookkeeping (where handlers live), re-resolve on hover-path change,
  and send only when the RESOLVED cursor differs from the last sent -
  never per pointer-move.
- packages/core types.d.ts: the `cursor` prop on the shared element
  props.

Traps, known up front:

- Pointer lock already hides and confines the cursor (SDL relative
  mode); while locked, suppress cursor sends and re-resolve on unlock.
- Touch has no cursor: resolution keys off mouse hover only (the hover
  path core tracks is already mouse-driven).
- `SystemCursor` in the sdl3 wrapper exposes 12 of SDL's 20 shapes
  (no per-direction resize cursors); adding `nw-resize` etc. means
  going through sdl3-sys, a separate decision - the 12 cover the CSS
  set apps actually use.
- The default when the hover path has no prop is an explicit
  `SetCursor(Default)` send (once, on transition), not "send nothing":
  the cursor is window state and must recover from whatever the last
  hovered element set.
