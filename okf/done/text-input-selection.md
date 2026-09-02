---
title: TextInput range selection
description: The text buffer already models an anchor/focus selection, but TextInput never grows one - no shift+movement, no drag, no highlight, no select-all, no delete-selection path from the UI - so copying or replacing a stretch of text is impossible; wire the gestures and keys onto the buffer's selection and draw it from the editor layout's line stops.
created: 2026-08-18
completed: 2026-09-02
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
skin was missing: it never passed `extend`, never set a range from pointer
input, and drew one caret rect.

## Done (2026-09-02)

All in the shared `EditorField` shell, so `TextInput` and `RichTextEditor`
both got it; the buffer needed no changes.

- Core: `selectionRects(anchor, focus)` on `createTextEditorLayout`, one
  box per touched line from the same line stops the caret uses. A line the
  selection continues past highlights to its ink end plus a space width
  (the `measureText(" ")` the layout already uses for the blank line), so
  selected newlines, crossed wraps and empty lines are visible; a range
  ending on a wrap boundary owns a zero-width box on the next line and
  drops it.
- Keys: Shift+Arrow/Home/End/Up/Down extend (the multiline visual-line
  paths keep the anchor through `setSelection`), Ctrl/Cmd+A selects all.
  Typing/Backspace/Delete on a range replace it - the buffer's existing
  behavior, now reachable.
- Pointer: Shift+tap extends; a mouse/pen down arms a drag whose first
  move steals the gesture arena (precise pointers select text over an
  enclosing scroller's pan, as desktop editors do), mapping moves through
  the exact frozen-path locals. Touch never arms - a finger drag still
  scrolls; touch selection is handle-based, split to
  [text-selection-touch-word](text-selection-touch-word.md).
- Highlight: one `d-rect` per selection rect behind the lines, drawn only
  while focused, in the new `theme.color.selection` token (optional in a
  theme definition, defaulting to `overlayPressed` - a neutral tint that
  needs no color math in the headless token build; the preset pair is
  translucent primary). Caret stays at the focus end.

Verified headless on probes/multiline-probe.tsx over the control API:
selection rects and replacement values for every key path, drag across
lines, Shift+tap, the touch no-select policy, blur hiding; a 30-move drag
showed 0 missed presents and 0 slow frames.

Clipboard stays out of scope here (no clipboard module yet); double-tap
word selection and touch handles moved to
[text-selection-touch-word](text-selection-touch-word.md).
