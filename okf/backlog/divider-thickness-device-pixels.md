---
title: A divider is 1 or 2 device pixels thick by its row at fractional scales
description: A divider is a filled rect in a view one logical pixel high, so the box snap rounds its two edges apart and at 1.5x it is 2 device pixels on one row and 1 on the next; its thickness is a stroke width and not a box size, so the divider becomes a line across its frame, and a line whose length is the frame's takes the stroke floor of border-width-device-pixels and lands its edges on the grid.
created: 2026-11-10
---

# A divider is 1 or 2 device pixels thick by its row at fractional scales

## Symptom

`Divider` is a `<view>` with a height of `thickness` (1 by default) and a
`<rect detached>` that fills it. The paint walk snaps that box to the
device grid, each edge by itself (okf/done/pixel-snapped-paint-boxes.md).
At 1.5x the box is 1.5 device pixels high, and its two edges round apart
or together by where the box starts: 2 device pixels when the top sits
under half a pixel past a whole one, 1 otherwise. A box at logical y 0 is
2 device pixels, one at y 1 is 1. The 1 px divider of
`alloy/examples/pixel_snap.rs` at 16.98 is 2. In a list the dividers
change thickness from row to row.

Each edge is crisp, so nothing that looks for a partial pixel sees it.

This is not the stroke of okf/done/border-width-device-pixels.md: there
the width is a stroke width that no snap reaches, here it is a box size
that the snap reaches twice.

## Cause

A divider's thickness is a width: one number that should paint the same
on every row. A filled box does not say that. Its thickness is the
distance between two edges, and the box snap is right to round each edge
where its neighbour's edge rounds, or boxes would leave seams.

The primitive that has a width is the line. Today it cannot stand in:

- A line is free-form geometry and nothing rounds it. Its stroke is
  centered on the segment and painted as declared. A 1 px line at 1.5x is
  1.5 device pixels wide: one whole row and one half-covered row, on
  every row.
- `packages/core/src/types.d.ts` calls a laid-out `<line>` in a thin box
  "practically a rule". By the endpoint rule (`Line::endpoints`) it draws
  the diagonal of that box, so a rule 200 long and 1 thick runs from
  (0, 0) to (200, 1): a slant of one stroke width, centered on the top
  left corner at one end and the bottom right at the other.

A rule across the frame can be written today, with no length: a detached
line takes each endpoint it does not get from the frame, so
`<line detached y1={t / 2} y2={t / 2} strokeWidth={t}>` runs from x 0 to
the frame's width through the middle of a frame `t` high. What is missing
is the grid.

## Done looks like

A two-point line is a rule when it is horizontal or vertical and its
length is the frame's: `y1` equals `y2` with `x1` and `x2` not set, or
`x1` equals `x2` with `y1` and `y2` not set, and `x`, `y` and `points` are
not set. Where the frame is axis-aligned with a uniform scale, a rule is
painted on the grid:

- Its width is the declared width floored to whole device pixels, never
  below one: the floor of okf/done/border-width-device-pixels.md, with
  that note's rules for a scale transition, a held write, a track on the
  width and `pixelSnap={false}`.
- The band it covers starts where the declared band starts (the center
  less half the declared width), rounded to a device pixel. The floored
  width runs from there. Where the rounding is off (a running track that
  moves the frame, or `pixelSnap={false}`) the start is not rounded and
  the width keeps its floor.
- A running track on one of the line's own endpoints leaves it as
  written for the run.
- A line with a length of its own, a slanted line and a polyline stay as
  written. They are the drawing and animation lane, as a detached rect
  with geometry of its own is.
- Hit testing and the reported bounds keep the declared geometry.
- A recording that holds a rule re-records when its scale changes, as one
  that holds a floored stroke does.

`Divider` draws a rule in place of its rect, horizontal or vertical by its
`orientation`. Its view keeps the declared thickness, so layout does not
change. At 1.5x a 1 px divider is 1 device pixel on every row, at the near
edge of its box (the top, or the left).

`alloy/examples/pixel_snap.rs` asserts it: a rule in a 1 px high view at
1.5x is 1 device pixel thick with no partial pixel, on a row where the box
snaps to 1 device pixel and on a row where it snaps to 2.

## Accepted

The floor is paint only, and unlike a border a divider takes layout space.
Its box is 1 or 2 device pixels by its row and the rule covers 1, so on
some rows one device pixel of the box shows what is behind the divider.
Over a shared surface that is a row pitch that changes by a pixel, which
the box snap already gives every row. Between rows with a background of
their own it is a strip of the surface below, on some rows and not on
others.

## What it involves

It builds on okf/done/border-width-device-pixels.md, which is in: the
`grid.rs` helper for the whole-device-pixel width at the scale the stroke
rests at (`grid::stroke_width`), and the count of content made for one
scale that a recording reads (`BuildContext::scale_bound`).

`kinds/line.rs`, in `build_geometry` for the two-point form: when the line
is a rule and the helper answers, draw the segment at the floored width
with its center at the rounded band start plus half that width. The band
start is rounded with `grid::shift`, which leaves it alone where the
rounding is off. The ends need nothing: a detached child draws under the
offset that rounds its frame's origin, and the size it inherits is the
snapped one (okf/done/pixel-snap-at-draw.md). A cap other than butt
reaches past the ends as it does today.

The rounding moves the band by up to half a device pixel past the declared
band. Damage holds that already (`DAMAGE_PAD_PX`, as for a snapped box).

The build counts a rule where the rect build counts a floored stroke, so
the recording rule of the border note covers it with nothing more.

The walk tells the build whether a track runs on the node's endpoints or
width, the same way the border note has it tell the rect build about
`StrokeWidth`.

`packages/components/src/divider.tsx`: the `<rect detached>` becomes a
`<line detached>` with `strokeWidth` set to the thickness and the two
cross-axis endpoints at half of it. The color and its transition move over
as they are.

`packages/core/src/types.d.ts` and `packages/core/AGENTS.md`: the line
comment stops calling a laid-out line in a thin box a rule, and names the
detached rule form and what it snaps.

## Open

Whether a laid-out `<line>` with no points should be a rule by itself: run
along the longer side of its box, through the middle of the shorter one,
in place of the diagonal. A `<line>` in a thin box is then the rule the
types call it, and `Divider` is one element with no view around it. It
changes what a laid-out line draws in a box that is not thin, and a square
box has no longer side. The divider does not need it.

## Rejected

- Snap the thickness ahead of layout, so the divider is laid out at a
  whole number of device pixels (0.67 logical px at 1.5x). The box snap
  then gives the same thickness on every row and no strip is left. It is
  where Chrome ended for borders. Layout here does not know the grid: the
  display scale and a design-size fit are static, a scale above the
  divider is not, and no component reads either today.
- Keep the rect and floor its thin axis. A box does not say which of its
  sides is a width, so this needs a rule for when a box counts as thin.

## Not part of it

The text caret of `editor-field.tsx` is a 1 px wide detached rect with
geometry of its own at a fractional x, so at 1.5x it is 1.5 device pixels
wide and soft. It is the same kind of width, but it has a length of its
own and moves with the text, so neither this rule nor the border floor
reaches it.

## Related

- okf/done/border-width-device-pixels.md: the stroke floor this reuses.
- okf/done/pixel-snapped-paint-boxes.md: the box snap that gives the
  divider its two thicknesses.
- okf/done/pixel-snap-at-draw.md: the frame a detached child inherits, and
  when the rounding is off.
