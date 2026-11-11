---
title: A divider and the text caret land off the device grid at fractional scales
description: A divider is a filled rect in a view one logical pixel high, so the box snap rounds its two edges apart and at 1.5x it is 2 device pixels on one row and 1 on the next, and the text caret is a 1 px detached rect at a fractional x that nothing rounds, so it is soft at any scale; both are a thickness that is a width and not a box, so both become a line, and a horizontal or vertical two-point line takes the stroke floor of border-width-device-pixels and lands its edges on the grid.
created: 2026-11-10
---

# A divider and the text caret land off the device grid at fractional scales

## Symptom

Two stock parts draw a thin rule as a filled box, and each is wrong in its
own way.

**The divider.** `Divider` is a `<view>` with a height of `thickness` (1 by
default) and a `<rect detached>` that fills it. The paint walk snaps that
box to the device grid, each edge by itself
(okf/done/pixel-snapped-paint-boxes.md). At 1.5x the box is 1.5 device
pixels high, and its two edges round apart or together by where the box
starts: 2 device pixels when the top sits under half a pixel past a whole
one, 1 otherwise. A box at logical y 0 is 2 device pixels, one at y 1 is
1. The 1 px divider of `alloy/examples/pixel_snap.rs` at 16.98 is 2. In a
list the dividers change thickness from row to row. Each edge is crisp, so
nothing that looks for a partial pixel sees it.

**The text caret.** `editor-field.tsx` (under `TextInput` and
`RichTextEditor`) draws the caret as a `<rect detached>` with `x`, `y`,
`w` (`CARET_WIDTH`, 1) and `h` of its own. A detached shape with geometry
of its own is painted as written, and the caret's `x` is where shaping put
the glyph edge, a fraction. So the caret is not on the grid at any scale:
at 1x its one pixel is spread over two columns, and at 1.5x it is 1.5
device pixels wide at a fractional column. Read from the code, not
rendered.

Neither is the stroke of okf/done/border-width-device-pixels.md: there the
width is a stroke width that no snap reaches. Here the divider's is a box
size that the snap reaches twice, and the caret's is a box size that the
snap does not reach at all.

## Cause

The thickness of a divider or a caret is a width: one number that should
paint the same wherever the rule sits. A filled box does not say that. Its
thickness is the distance between two edges. For a laid-out box the snap
is right to round each edge where its neighbour's edge rounds, or boxes
would leave seams. For a detached box with geometry of its own nothing
rounds, because that is the animation lane.

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

Both rules can be written today as a line. A detached line takes each
endpoint it does not get from the frame, so
`<line detached y1={t / 2} y2={t / 2} strokeWidth={t}>` runs from x 0 to
the frame's width through the middle of a frame `t` high, with no length
given: the divider. A caret is a vertical segment with all four endpoints.
What is missing is the grid.

## Done looks like

A two-point line is a rule when it is horizontal or vertical: `y1` equals
`y2`, or `x1` equals `x2`, after the frame fills in the endpoints that are
not set, and `points` is not set. Where the frame is axis-aligned with a
uniform scale, a rule is painted on the grid:

- Its width is the declared width floored to whole device pixels, never
  below one: the floor of okf/done/border-width-device-pixels.md, with
  that note's rules for a scale transition, a track on the width and
  `pixelSnap={false}`.
- The band it covers starts where the declared band starts (the center
  less half the declared width), rounded to a device pixel. The floored
  width runs from there.
- Its two ends are each rounded to a device pixel, as the far edges of a
  box are. A rule whose length is the frame's has its ends on the grid
  already.
- Where the rounding is off (a running track that moves the frame, or
  `pixelSnap={false}`) the band start and the ends are not rounded and
  the width keeps its floor.
- A running track on one of the line's own endpoints, or on its `x` or
  `y`, leaves it as written for the run.
- A slanted line and a polyline stay as written.
- Hit testing and the reported bounds keep the declared geometry.
- A recording that holds a rule re-records when its scale changes, as one
  that holds a floored stroke does.

`Divider` draws a rule in place of its rect, horizontal or vertical by its
`orientation`. Its view keeps the declared thickness, so layout does not
change. At 1.5x a 1 px divider is 1 device pixel on every row, at the near
edge of its box (the top, or the left).

The caret is a vertical rule at the caret's x, in place of its rect. At
any scale it is a whole number of device pixels wide (1 at 1x and at
1.5x), on whole columns, with whole ends. Caret movement, the blink and
the scroll still touch no layout, and the column `CARET_WIDTH` reserves in
the scroll offset does not change.

`alloy/examples/pixel_snap.rs` asserts it: a rule in a 1 px high view at
1.5x is 1 device pixel thick with no partial pixel, on a row where the box
snaps to 1 device pixel and on a row where it snaps to 2; and a vertical
rule with endpoints of its own at a fractional x is 1 device pixel wide
with no partial pixel.

## The rule this changes

"A detached shape with geometry of its own is painted as written" is a
decision of okf/done/pixel-snapped-paint-boxes.md, kept by
okf/done/pixel-snap-at-draw.md: such shapes are the drawing and animation
lane. This item takes the horizontal and the vertical two-point line out
of it.

- It follows the rule boxes already have: a value written from JS is not
  motion and stays rounded, a running track drops the rounding, and
  `pixelSnap={false}` turns it off for a subtree.
- What changes for an app that draws such lines today: the line sits up
  to half a device pixel from where it was written and its width is
  floored. A chart's grid lines and axes get crisp. A horizontal or
  vertical line that JS moves frame by frame steps by device pixels,
  until it takes a transition or `pixelSnap={false}`
  (okf/backlog/pixel-snap-scroll-and-js-motion.md asks about motion
  written from JS as a whole).
- No stock component draws a `<line>` today.

The narrower rule, only a line whose length is the frame's, keeps the
decision whole and is enough for the divider. It does not reach the caret:
a caret would need a frame of its own to take its length from, a frame of
its own is a laid-out box, and caret movement must not touch layout.

## Accepted

The floor is paint only, and unlike a border a divider takes layout space.
Its box is 1 or 2 device pixels by its row and the rule covers 1, so on
some rows one device pixel of the box shows what is behind the divider.
Over a shared surface that is a row pitch that changes by a pixel, which
the box snap already gives every row. Between rows with a background of
their own it is a strip of the surface below, on some rows and not on
others.

The caret can sit up to half a device pixel from the glyph edge it marks.

## What it involves

It builds on okf/done/border-width-device-pixels.md, which is in: the
`grid.rs` helper for the whole-device-pixel width at the scale the stroke
rests at (`grid::stroke_width`), and the count of content made for one
scale that a recording reads (`BuildContext::scale_bound`).

`kinds/line.rs`, in `build_geometry` for the two-point form: when the line
is a rule and the helper answers, draw the segment at the floored width
with its center at the rounded band start plus half that width, and its
two ends rounded along its axis. The band start and the ends are rounded
with `grid::shift`, which leaves them alone where the rounding is off. For
a rule whose length is the frame's the ends move nothing: a detached child
draws under the offset that rounds its frame's origin, and the size it
inherits is the snapped one (okf/done/pixel-snap-at-draw.md). A cap other
than butt reaches past the ends as it does today.

The rounding moves the band and the ends by up to half a device pixel past
the declared geometry. Damage holds that already (`DAMAGE_PAD_PX`, as for
a snapped box).

The build counts a rule where the rect build counts its stroke, so the
recording rule of the border note covers it with nothing more.

The walk tells the build whether a track runs on the node's endpoints,
offsets or width, the same way it tells the rect build about
`StrokeWidth` (`BuildContext::stroke_width_moving`).

`packages/components/src/divider.tsx`: the `<rect detached>` becomes a
`<line detached>` with `strokeWidth` set to the thickness and the two
cross-axis endpoints at half of it. The color and its transition move over
as they are.

`packages/components/src/editor-field.tsx`: the caret's `<rect detached>`
becomes a `<line detached>` with `x1` and `x2` at the caret's x plus half
of `CARET_WIDTH`, `y1` and `y2` at the rect's top and bottom, and
`strokeWidth` at `CARET_WIDTH`.

`packages/core/src/types.d.ts` and `packages/core/AGENTS.md`: the line
comment stops calling a laid-out line in a thin box a rule, and says that
a horizontal or vertical two-point line is painted on the device grid and
how to turn that off. `docs/guides/animation.md` names it where it
describes what is drawn on whole device pixels.

## Open

Whether a laid-out `<line>` with no points should be a rule by itself: run
along the longer side of its box, through the middle of the shorter one,
in place of the diagonal. A `<line>` in a thin box is then the rule the
types call it, and `Divider` is one element with no view around it. It
changes what a laid-out line draws in a box that is not thin, and a square
box has no longer side. Neither the divider nor the caret needs it.

## Rejected

- Snap the thickness ahead of layout, so the divider is laid out at a
  whole number of device pixels (0.67 logical px at 1.5x). The box snap
  then gives the same thickness on every row and no strip is left. It is
  where Chrome ended for borders. Layout here does not know the grid: the
  display scale and a design-size fit are static, a scale above the
  divider is not, and no component reads either today. It does nothing
  for the caret.
- Keep the rects and floor a thin axis. A box does not say which of its
  sides is a width, so this needs a rule for when a box counts as thin.

## Not part of it

The selection highlight of `editor-field.tsx`: filled detached rects with
geometry of their own, so their edges are as soft as the caret is today.
They are boxes and not widths, and stay in the lane this item leaves
alone.

## Related

- okf/done/border-width-device-pixels.md: the stroke floor this reuses.
- okf/done/pixel-snapped-paint-boxes.md: the box snap that gives the
  divider its two thicknesses, and the decision on detached geometry.
- okf/done/pixel-snap-at-draw.md: the frame a detached child inherits, and
  when the rounding is off.
- okf/backlog/pixel-snap-scroll-and-js-motion.md: motion written from JS.
