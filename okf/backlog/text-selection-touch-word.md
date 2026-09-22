---
title: Touch and word text selection
description: Text selection exists (keys, mouse drag, highlight) but a touch user cannot make one at all - a finger drag deliberately scrolls - and no pointer selects a whole word; add long-press-to-select with draggable handles on touch, and double-click word selection for the mouse.
created: 2026-09-02
---

# Touch and word text selection

## Symptom

On a phone or tablet there is no way to select text in a `TextInput`: a
finger drag scrolls (by design - see
[text-input-selection](text-input-selection.md), where touch was kept out
of the drag-select arena), Shift+tap needs a keyboard, and long-press does
nothing. And on any pointer, selecting one word takes a careful drag;
editors do it with a double-click.

## Done looks like

- Touch: long-press on the text selects the word under the finger and
  shows a start and end handle; dragging a handle adjusts that end of the
  selection (the anchor is the other handle, not the press point). Tap
  elsewhere collapses.
- Mouse/pen: double-click selects the word under the cursor;
  double-click-and-drag extends by whole words is a nice-to-have.
- A "word" comes from the editor layout's shaped units (the wrap units the
  line breaker already segments), not a new segmenter.

## Roughly

Component-only over the existing selection machinery in `EditorField` plus
`selectionRects`. The recognizers exist since 2026-09-22 (core's
`createLongPress`, which wins the finger at its timer and streams moves
after it, and `createDoubleTap`; okf/done/pointer-discrete-gestures.md);
what remains is handle drawing (two small `d-path`
grips at the first/last selection rect corners) with their own drag
mapping through `offsetAtX`/`lineAtY`. Clipboard integration (the usual
reason to select on touch) is the separate clipboard module.
