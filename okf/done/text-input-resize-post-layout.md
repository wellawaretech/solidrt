---
title: A TextInput whose box changes each frame still costs ~2.4 ms per empty field in the post-layout flush
description: Fixed 2026-09-24: the placement re-read the caller's input() object (some twenty reactive reads) on every viewport width change, and each width and height write marked the field's whole graph for a re-check that found nothing; the break inputs now come through a memo of their own, and the layout handler writes a width only outside the range the current lines hold for and a height only when the retained scroll would move. Nine empty multiline fields in resizing panes on the Galaxy Tab A7: postLayout 24 ms to 1.6 ms, 20 fps to 60.
created: 2026-09-22
completed: 2026-09-24
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

## Fixed (2026-09-24)

Measured first, on the desktop client with counters in
`createTextEditorLayout` (packages/core/src/text-input.ts): per field per
frame, `placed` and `scrollY` recomputed and nothing downstream did (the
equality gates already held), and of the ~80 us that cost, 60 were
`input()` - the caller builds that object per read (value, font with its
theme and policy reads, runs, caret), and `placed` read it for three
fields. Then three changes:

- `breaking`, a memo of `{ text, wrap, caretWidth }` equal by field, is what
  `placed` and `scrollX` read; `input()` is read once per real change.
- Each placement carries `holds`, the wrap widths [min, max) at which the
  greedy breaker yields the same lines (every line's ink fits, no line's
  first unit fits at the end of the line before it, except across a hard
  break); the layout handler writes `viewportWidth` only when the box's
  width leaves that range. Unwrapped fields always write it, since it is
  the horizontal scroll's extent.
- The height is written only when `follow` at the new height would move the
  retained scroll offset.

So the signals lag the box while nothing they gate can change, and a
placement or scroll re-run for a text or caret change at a lagging value is
checked again in that frame's layout handler and corrected before paint
(writes from onLayout land in the same frame's re-layout). Verified on the
desktop: a typed field re-breaks from 5 to 8 lines when its pane narrows
and back, the scroll follows the caret through both, typing past the box
scrolls.

Nine empty multiline fields in panes sliding between two sizes on the
SM-T500 (`srt android --census` and `get_stats`): postLayout 24.1 ms, 41
presents over 2.3 s at 50 ms p50, before; 8.3 ms after the memo alone; 1.6
ms, 119 presents at 16.7 ms p50 (114 at one refresh) with the gates, the
same as nine fixed-size fields. performance.md carries the rule.
