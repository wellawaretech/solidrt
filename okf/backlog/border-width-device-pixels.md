---
title: Border widths land off the device grid at fractional scales
description: A 1 logical px stroke at 1.5x is 1.5 device pixels, so its inner edge sits on a half pixel and reads as a soft second row whatever the box does; a rect's stroke is floored to whole device pixels (at least one), as Chrome's is, for a laid-out rect and for the detached rect that draws every stock border.
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
a 2 px stroke at 1.5x.

Every stock border is a detached rect: `View`, `Pressable`, `ScrollView`,
`Image` and `Surface` draw theirs as a `<rect detached drawStyle="stroke">`
that fills the frame of the view it sits in. So the case that matters is the
detached one.

## Done looks like

A Rectangle's stroke (an Oval's is curved and stays as it is) is painted at
a whole number of device pixels when the frame is axis-aligned with a
uniform scale: the declared width floored to device pixels, never below
one.

- It applies when the rect takes its whole geometry from the frame: none
  of `x`, `y`, `w` and `h` is set. That is every laid-out rect, and a
  detached rect that fills the frame it inherits. A detached rect with
  geometry of its own is the animation lane and stays as written.
- Movement does not change the width.
- While a track runs on the width itself the stroke is not floored, and
  the floor returns when the track settles.
- Hit testing keeps the declared width.
- `alloy/examples/pixel_snap.rs` asserts a 1 px stroke at 1.5x with no
  partial pixel, for a laid-out rect and for a `<rect detached>` border in
  a view.

Floor, not round: a 1 px border at 1.5x is 1 device pixel, which is what a
1 px hairline usually means. Chrome floors; Firefox rounds.

## What it involves

A new `grid.rs` helper gives the whole-device-pixel width, for a frame that
is axis-aligned with `m11 == m22`. `grid::scale` alone does not qualify: it
answers under a rotation and takes the longer axis. `kinds/rect.rs` uses the
result in `stroke_path` and `dashed_outline`, `stroke_inset` takes it
instead of the declared width, and the paint is built with it. A zero width
still paints nothing. The floor takes a named epsilon, so matrix noise
(3.9999 for 4) loses no pixel.

The floor is taken at the scale the stroke rests at, the grid scale times
`ctx.scale_target`, so a scale transition carries one logical width from
start to end and nothing pops when it settles. A translate or a scroll
leaves `scale_target` at 1.

The floor reads one transition only: a running track on the node's
`StrokeWidth` skips it, as the width is what moves. The settling tick
writes the end value, so that frame is floored. A held write does not skip
it. `any_running` counts held writes too, so this needs a query of the
tracks alone (okf/done/pixel-snap-at-draw.md adds the same one).

`Rectangle::build` serves laid-out and detached rects alike. It needs
nothing from the walk to tell them apart: its own `x`, `y`, `w` and `h`
say whether the geometry is the frame.

## Related

- okf/done/pixel-snapped-paint-boxes.md: the box snap this completes.
- okf/done/pixel-snap-at-draw.md: independent. The frame a detached rect
  inherits is on the grid today and stays there after it. Under its running
  tracks the box is not rounded, and the width keeps its floor.
