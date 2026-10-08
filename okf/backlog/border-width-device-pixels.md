---
title: Border widths land off the device grid at fractional scales
description: A 1 logical px stroke at 1.5x is 1.5 device pixels, so its inner edge sits on a half pixel and reads as a soft second row whatever the box does; browsers snap border widths to whole device pixels (at least one), and the box kinds' strokes should too.
created: 2026-10-08
---

# Border widths land off the device grid at fractional scales

## Symptom

The paint walk snaps every axis-aligned box to the device grid
(okf/done/pixel-snapped-paint-boxes.md), so a stroked box's outer edge is
crisp at any scale. The stroke's inner edge is one stroke width in, and
at 1.5x (the tablet) a 1 logical px stroke is 1.5 device pixels: the
inner edge sits on a half pixel and the border reads as one solid row and
one half-covered row. `alloy/examples/pixel_snap.rs` works around it with
a 2 px stroke at 1.5x. Chrome snaps border widths to whole device pixels
(floor, at least one device pixel); Firefox rounds.

## Done looks like

A box kind's stroke (Rectangle; an Oval's is curved and stays as it is)
is painted at a whole number of device pixels when the frame is
axis-aligned with the grid: the declared width rounded to device pixels,
never below one. Hit testing keeps the declared width. The pixel_snap
example asserts a 1 px stroke at 1.5x with no partial pixel.

The decision to take first: round or floor. Rounding keeps a 1 px border
at 1.5x closer to its declared size (2 device pixels); flooring keeps it
thinner (1 device pixel), which is Chrome's choice and what a 1 px
hairline usually means.

## Involves

`kinds/rect.rs`: `stroke_path` and `dashed_outline` take the snapped
width, from `grid::scale(&ctx.grid)` in `build`; `kinds/paint.rs` if the
stroke width is resolved there.
