---
title: A TextInput whose box changes each frame still costs ~2.4 ms per empty field in the post-layout flush
description: With nine empty multiline fields inside panes sliding and resizing on a layout transition, the SM-T500 spends 22 ms per frame in postLayout (handlers 0.7 ms, the flush after them the rest) after the same-breaks fix; sizing each field to its settled tile so its box never changes mid-slide drops it to 1 ms. The field should skip its geometry work when the box change re-breaks nothing.
created: 2026-09-22
---

# A TextInput whose box changes each frame still costs ~2.4 ms per empty field in the post-layout flush

## Symptom

demo/notes, Galaxy Tab A7, release client 0.0.60-10-gb1eec1bf (which
includes the "equal placements keep the previous value" change in
core/src/text-input.ts). Nine empty `<TextInput multiline>` fields, one per
pane, panes sliding and resizing on `transition={{ layout }}`. Per frame
of the slide, from the slow-frame log and get_stats: js 0.2, layout 3.5,
postLayout 21-22, paint 3.7 ms. Every field's viewport box changes every
frame (its pane is resizing), and the flush after the layout handlers is
what costs: the app's own probe measured handlers at 0.7 ms and the flush
at 17-27 ms.

Giving each field an explicit `width`/`height` (the settled tile's size,
with `flexShrink: 0`) so its box does not change during the slide:
postLayout 0.9 ms, JS critical path p50 5 ms.

## What is still running

`setViewportWidth`/`setViewportHeight` fire for each field each frame;
`placed` recomputes and keeps its value (same breaks), but everything
keyed on the height (`scrollY`, the viewport's `height={viewportHeight()}`
render effect, the caret) recomputes, and `input()` rebuilds its object
per read. Multiplied by nine fields on an interpreted engine this is 20
ms; on desktop it is invisible.

## What done looks like

- A field whose box changes but whose breaks and content do not re-runs
  nothing downstream: gate the height-dependent memos on an actual change
  of the values they read, or make the viewport size one signal with a
  shallow-equal object so a height-only change touches only scroll.
- performance.md notes that a laid-out field inside a resizing box pays
  per frame, and that a fixed box (or a paragraph while not editing) is
  the way out.
