---
title: TextInput range selection
description: The text buffer already models an anchor/focus selection, but TextInput never grows one - no shift+movement, no drag, no highlight, no select-all, no delete-selection path from the UI - so copying or replacing a stretch of text is impossible; wire the gestures and keys onto the buffer's selection and draw it from the editor layout's line stops.
created: 2026-08-18
---

# TextInput range selection

## Symptom

A user cannot select text in a `TextInput`: Shift+Arrow, Shift+Home/End,
Shift+tap, dragging over the text and Ctrl/Cmd+A do nothing but move (or
not move) the caret, and no highlight is ever drawn. Deleting or replacing
a phrase means backspacing over it. Copy/paste has nothing to act on.

## Why

`createTextBuffer` (packages/core/src/text-input.ts) owns a `Selection`
`{ anchor, focus }`, `move(direction, { extend })`, `setSelection`, and its
edits already replace a range. `createTextEditorLayout` gives every line's
caret stops (`offsetAtX`, `lineAtY`, `caret()`), which is exactly what a
highlight needs (per-line rects between two stops). Only the component
skin (packages/components/src/text-input.tsx) is missing: it never passes
`extend`, never sets a range from pointer input, and draws one caret rect.
Tap-to-position (2026-08-18) put the pointer plumbing in place: the
viewport handler maps a pointer to an offset.

## Done looks like

- Shift+Left/Right/Home/End/Up/Down extend the selection (buffer `extend`).
- Pointer down + drag (or Shift+tap) selects a range; drag ends the
  selection at the pointer's offset via the same `lineAtY`/`offsetAtX`.
- Ctrl/Cmd+A selects all.
- Highlight drawn as one `d-rect` per touched line behind the text (theme
  color, e.g. primary at low alpha); caret at the focus end.
- Typing/Backspace/Delete on a range replaces/deletes it (already the
  buffer's behaviour, just reachable now).
- Word selection on double-tap is a nice-to-have, not required.

## Roughly

Component-only for the keys and highlight; core gets a small helper on the
editor layout for the range rects if the per-line arithmetic wants to live
next to `lineStops` (it should: `selectionRects(start, end)`). Drag needs
the pointer captured on the field the way ScrollView's pan does, or the
frame-batched move path ([frame-batched-pointer-input](../done/frame-batched-pointer-input.md):
moves never travel as events - read the pointer position per frame). Clipboard is out of scope here (no clipboard module yet).
