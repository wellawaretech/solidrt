---
title: A lean program for rounded rect fills
description: A rounded rect fill goes through the shape program, whose per-fragment cost on the Mali-T860 (the kind branches, the discard, the derivatives, the nine varyings) makes thirty gradient panels of 320x180 device pixels cost about 20 ms of GPU a frame against about 2 ms as plain rects; a fill-only rounded program (the rounded-box distance, no stroke, no oval or line arm, no texture), also taking plain fills under a rounded clip, would bring panels, cards and buttons near the fill program's cost.
created: 2026-11-19
---

# A lean program for rounded rect fills

Stage 2's fill program taught that the fragment program's shape, not the
instance count, sets the Mali's cost, and gave plain rect fills a program
of their own. Rounded rects, the most common shape in a UI after plain
rects (every card, panel, button, chip), still draw through the full
shape program. The stage 4 TV read (okf/plans/own-renderer.md) priced
it: thirty 320x180 device-pixel panels (1.7 million fragments) as gradient
rounded rects cost about 33 ms of GPU a frame, as gradient plain rects
15.6 ms, as solid plain rects 14.1 ms, so the rounded corners cost about
18 ms where the gradient itself costs 1.5 ms.

The same cost reaches plain rects under a rounded clip: `plain_fill`
(walk.rs) keeps any instance with clip radii out of the fill program, so a
card whose corners come from `overflow="hidden" clipRadius` sends its fill
and every plain rect inside it through the shape program.
rounded-clip-cost-android.md is that case on the tablet, measured under
Impeller; under anvil its remaining cost is most likely this one.

## Done looks like

- A rounded rect fill (untextured, unstroked) draws through a program
  with the rounded-box distance and coverage only: no kind branch, no
  texture sample, the clip box as a factor and `coverage()` multiplied
  in, a discard nowhere. A gradient variant beside it.
- A plain rect fill under a rounded clip draws through the same program:
  it is the same rounded-box distance, applied to the clip instead of the
  shape (the quads' `roundedClip()`), so a clipped card's fills stay off
  the shape program.
- The path bench's static route with rounded gradient panels reads near
  the plain-rect figure on the TV; pixels unchanged (zero diff). The same
  for plain panels under a rounded clip.

## Involves

- A fourth batch class in `walk.rs` (`BatchProgram::Rounded`, or the
  fill class with a rounded flag) and the fold rule: a rounded rect
  rides along in a shape batch below an area threshold, as a plain fill
  does, else opens its own.
- The derivatives: `coverage()` needs `footprint()`; keeping that but
  dropping the branches and the discard is the experiment, measured on
  the TV as the fill program was.
