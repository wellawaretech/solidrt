---
title: An inset option on grid and namedFrames for hand-packed atlases without gutters
description: Frames addressed as whole-pixel rects that share an edge bleed a one-texel line of the neighbouring cell on the odd frame of any fractional motion; the fix is a half-texel inset or a gutter, and both slicers are the place to apply it once instead of in every app's rect table.
created: 2026-09-07
---

# An inset option on grid and namedFrames

## Symptom

A sheet packed edge to edge and sliced with `namedFrames` (or `grid`
without `spacing`) draws correctly while sprites sit on whole pixels.
The moment one drifts by fractions of a pixel - scenery scrolling at 40
px/s, a camera easing - a sample position along its edge rounds into the
cell next door, and a one-texel line of that cell paints along the
sprite's edge for a single frame. With a solid white cell beside a
sprite's cell that is a bright bar flashing over it, gone the next frame,
impossible to catch in a screenshot and easy to blame on the game.

The doc comments on both slicers now say so (landed 2026-09-07:
`packages/2d/src/frames.ts`), with the two remedies: inset every rect
half a texel (`[x + 0.5, y + 0.5, w - 1, h - 1]`) or pack a transparent
gutter and pass it as `spacing`. Both are the app's to apply, in every
rect of every table.

## Done looks like

One knob, applied by the slicer:

- `grid(atlas, cols, rows, { ..., inset?: number })` and `namedFrames(atlas,
  rects, { inset?: number })` in atlas texels, default 0. The UV rect
  shrinks by `inset` on every side. Half a texel is the value that stops
  the bleed with `filter: "nearest"`; a linear atlas wants a full texel
  (its 2x2 tap reaches one texel out).
- The trap paragraph in frames.ts then names the option instead of the
  arithmetic.

## What it involves

Six lines in `frames.ts` and a case each in `checks/frames-check.ts`
(the inset rect against the hand-computed UVs; a frame that would invert
throws, matching the non-positive-size check). No layer change: the
frame is normalized UVs either way.

Open: whether `createAtlas` should take the inset too, so a sheet
declares it once for both slicers. Probably not until a second consumer
asks - the slicers are where rects become UVs.
