---
title: A bounding box read under a sliding ancestor drifts in size
description: getBoundingBox computed a node's size from corners that already carried the ancestor chain's translation, so under a layout slide the width came back an ulp different every frame and every consumer comparing sizes re-ran its work per frame; done means the size depends only on the transforms, and the text editor sizes its wrap from the layout box.
created: 2026-09-21
completed: 2026-09-21
---

# A bounding box read under a sliding ancestor drifts in size

## Symptom

A grid of multiline `TextInput`s whose tiles glide on a `layout`
transition: for the whole slide, `postLayoutMs` grows with the number of
fields (several ms each on a low-end tablet) while `layoutMs` and
`dirtiedNodes` stay at zero, and `measureCalls` shows one call per field
per frame. Reported in [[quartz-heron]] item 4f.

## Cause

`createTextEditorLayout` (packages/core/src/text-input.ts) read its
viewport size from `getBoundingBox` in `onLayout` every frame and relied
on the `viewportSize` signal's equality to skip the line layout when
nothing changed. `RenderTree::compute_corners` carried the node's four
corners up the ancestor chain, adding every layout placement to them, and
`Rect::from_points` then took the width as `max.x - min.x`, that is
`(x + w + t) - (x + t)` in f32. That is exactly `w` only while `x + w + t`
stays in the same binade as `w`; once the sum crosses a power of two the
rounding of the sum is an ulp of the result, and it changes as the slide
offset `t` changes. So every field under a sliding tile read a fractionally
different width or height each frame, the equality failed, `placed()`
re-broke every line (with a `measureText` for the blank last line) and the
per-line `d-text`s were rewritten. Detached writes never dirty layout,
which is why the counters said nothing happened.

The same read also wrapped the text to the transformed width: under a
`scale` transition on the tile the lines re-broke at the scaled width on
every frame of the pop.

## Done

- `compute_corners` keeps the chain's translations (placements,
  `translate`, scroll) in a separate shift and folds them into the corners
  only when a matrix has to see them; `bounding_box` takes the AABB of the
  corners and translates the finished box, so its size depends only on the
  transforms in the chain, and `painted_quad` adds the shift per corner.
  Tests: `bounding_box_size_exact_under_fractional_placement` and the
  identity-scale variant in alloy/src/tests/tree.rs, with placements chosen
  past the binade edge.
- `createTextEditorLayout` sizes its wrap from `getLayoutBox` (the
  untransformed solved box, the frame the lines are drawn in), the scroll
  offsets are memos over the caret, the lines and the viewport size
  (`follow` from their previous value), and the `onLayout` handler only
  publishes the box; the post-layout drain in `runLayoutHandlers` is the
  one flush.

Verified on the reporting app: `measureCalls` 0 on every slide frame and
`postLayoutMs` below half a millisecond on the same tablet.
