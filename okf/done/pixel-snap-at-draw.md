---
title: Snap to the device grid once, at the draw
description: A scale transition around text wobbles by up to a device pixel per frame and steps when it rests, because the paint walk rounds a view's origin, the child box and the text quad in turn, each on top of the last; rounding moves to the draw, a running track that moves a box drops the rounding for its subtree until it settles, a text under a downward scale rasters for its end, and `pixelSnap={false}` turns the rounding off for a subtree that must not step when its motion ends.
created: 2026-11-08
---

# Snap to the device grid once, at the draw

## Symptom

A number that pops in with `transition={{ scale: { from: 1.18, duration:
180, curve: "ease-out" } }}` on a `<view>` around a `<text>` does not move
smoothly. Read frame by frame at display scale 2, the ink centre of the
digits runs 210.55, 209.73, 210.21, 210.03, 209.18, 209.64, 208.36, 209.70:
back and forth by up to a device pixel per frame. A
`repaintBoundary="snapshot"` view under the scaled view wobbles the same
way. When the scale comes to rest there is a visible step on the last
frame.

The same content as `Text2d` in a `SpriteLayer`, with the scale on a
`Group`, moves one way only, because the 2d path does not snap.

The case that shows in an app today is a title whose letters animate (a
game's menu): each letter a `d-view` with spring tracks on `y`, `scale`
and `rotate` around a `d-text`, under a 1.5 design-size fit at display
scale 1. Nothing rounds while the rotation is not zero. Read frame by frame
after a hover is released, the ink centre drifts smoothly to 275.11, moves
up 0.34 on the frame the `rotate` track settles (rounding comes back while
`y` and `scale` still run), and moves down 1.000 four frames later, when
the `y` track settles on a value that puts the letter's origin on a half
pixel. The same read with no scale change has the same whole-pixel move, so
it is the rounding and not the raster. To the eye the letter jumps a pixel
when its motion ends. The read is the first test of
`packages/core/tests/pixel-snap-motion.test.tsx`.

## Cause

`grid.rs` counts any positive scale as axis-aligned, so
okf/done/pixel-snapped-paint-boxes.md applies under a moving scale. For a
`<view>` around a `<text>`, three roundings run per frame, each computed
from the result of the one before:

1. `snapped_own` rounds the scaled view's origin. A scale about the centre
   moves that origin every frame.
2. The child loop of `record_node` rounds the text's box with
   `grid::snap_box` and places the subtree there.
3. `Text::build_layer` rounds the layer's quad with `grid::shift`.

The double rounding is wrong for static layout too: a parent at 10.4 with a
child at +0.3 belongs on 11 (10.7 rounded) and lands on 10.
`Tree::device_box` rounds the absolute box once, so it can disagree with
the paint by a pixel while documenting that it matches.

One rounding is not enough under a moving scale either. Only the box's
top-left corner lands on a pixel while the size keeps changing, so the
centre of a centre-scaled box still moves back and forth by up to 0.5
device px. The raster is being resampled under a moving scale anyway, so
rounding buys nothing there and only adds error: smooth motion needs no
rounding at all.

The step at rest is the raster, not the snap, and not hinting (`hint_at`
is `Hint::Off` at display scale 2). `raster_density` makes the first raster
at 2 x 1.18 = 2.36, keeps it minified while the transition runs down, and
re-rasters at 2.0 when the scale has held.

## Done looks like

- A box is rounded once, when it is drawn. Ancestors no longer shift their
  descendants, apart from a boundary's origin. A scroll offset, and a
  translate that no track is moving, are rounded as a movement first, so a
  box under one can land up to 1 device px from its ideal position (a
  parent at 10.4 with a `translate` of 0.3 lands on 10, not 11). That is
  accepted: it stays crisp and does not wobble. A boundary and a node
  capture round their interior from their own whole origin, so a nested
  child can land 1 device px from where the walk lands it without that
  boundary. That is accepted too.
- A detached child draws under the offset that rounds the origin of the
  frame it inherits. A `d-rect` that fills its parent, which is every stock
  background and border (`View`, `Pressable`, `Surface`, `ScrollView`,
  `Image`), covers the pixels a laid-out box of that size covers. Its own
  `x`, `y`, `w` and `h` stay as written, as today.
- The rule: if a running track moves it, snapping is off; when the track
  settles, snapping is back as it was. A value written directly from JS is
  not motion: it stays rounded, and to move smoothly it goes through a
  transition.
- A running track on a translate, a scale, a rotation, a transform origin,
  a perspective or a layout slide drops the rounding for its subtree with
  no prop. The frame-by-frame read of the symptom has no back-and-forth
  while the track runs, for the text and for a snapshot boundary under the
  scaled view. The settling frame is read apart (see below). A statically
  scaled view stays rounded, also when it repaints after idle.
- A delayed transition stays rounded for the length of its delay: a held
  write is not motion.
- A scroll offset moves its content by whole device pixels, under a
  transition too (okf/backlog/pixel-snap-scroll-and-js-motion.md asks
  whether that should change). A translate written directly does the same.
  A child never changes its distance to its parent. Zero movement lands
  where no movement lands, so a translate of 0 and an unscrolled scroller
  move nothing.
- A text under a downward scale transition within
  `LAYER_SCALE_MOTION_BOUND` of its end does not re-raster when the scale
  rests, on a first raster and on a re-triggered transition alike, so the
  step is gone. Inside a snapshot under that transition the text rasters at
  the snapshot's density.
- A recording made under a running track re-records once when the track
  ends, so its interior ends up on the resting grid, text or no text. A
  snapshot made under one re-rasters once when it ends if its box at rest
  is another size. A snapshot made at rest does not re-raster when a track
  starts or ends.
- `pixelSnap={false}` on any element turns the rounding off for the element
  and its subtree, in motion and at rest: no box, no text quad, no movement
  and no boundary origin in it is rounded. The title's letters under it end
  their motion where the motion put them: the frame-by-frame read has no
  step on any frame, the settling frames included.
- `alloy/examples/pixel_snap.rs` still holds, asserts a nested fractional
  box landing on its single-rounded pixel, and asserts no partial pixel for
  a `d-rect` that fills a view at a fractional position.

When a track settles, rounding resumes and the box moves once, by at most
0.5 device px (none on a whole-pixel resting position), and by up to 1
under a translate, which rounds as a movement on top (a letter translated
to its centre with its text offset back steps 0.94 in the test). An ease-out tween
or a spring moves less than that in its last frames, so the settling frame
can read as a small step against the trend. That is accepted: it is at most
half a device pixel and it happens once. Where it is not accepted, as for
a title that animates, the subtree takes `pixelSnap={false}` and has no
step at all. A downward transition past `LAYER_SCALE_MOTION_BOUND` keeps
today's raster behaviour until it is within the bound of its end.

## What it involves

The rule: only a boundary rounds a position ahead of the draw. Everything
else that moves a group rounds the movement, never the position it lands
on. A group moving by whole device pixels cannot change the fractions of
its children, so each child's single rounding at the draw stays stable and
neighbours keep their spacing.

A new `grid.rs` helper rounds a movement vector to whole device pixels of a
frame: `(v * scale).round() / scale` per axis, unchanged when the frame is
not axis-aligned or rounding is off.

In `composite::record_node`:

- The child loop places a laid-out child at its layout location plus its
  slide delta (`placement()` minus the layout location) rounded by the
  helper, so `builder.translate`, `ctx.grid`, `ctx.to_window` and the cull
  rect stay fractional. A slide moves the node here, ahead of the node's
  own matrix, so the loop reads the child's motion (see Rounding off) for
  its `snap_box` and its slide delta.
- `snap_box` still gives the snapped size, and `ctx.size` stays that size,
  so the box kinds do not change.
- The box offset is `grid::shift(&box_grid, Point::zero())`, read in
  `record_node`. Every kind's `element.build` runs under it, laid out or
  detached, except a text's. For a laid-out box kind (fill and stroke;
  these kinds have no own matrix) it lands the box. For a detached kind
  `box_grid` is the frame the node inherits, so the offset rounds that
  frame's origin, the node's own `x` and `y` add on top as written, and
  the size it inherits is the parent's snapped size. Today a detached child
  gets the same from the parent's snapped translate, which goes away. A
  view paints no box of its own: its clip and backdrop are emitted inside
  the own matrix under the offset, and it is undone before the scroll, the
  fit and the children. A boundary's box grid is whole, or rounding is off
  under a running track, so its offset is zero either way and the boundary
  paths take no offset of their own. A statically scaled view reads it
  through its matrix and lands its origin as it does today. Text skips the
  offset: its quad is already its one rounding, and `ctx.grid` has to
  describe where the builder is.
- A non-boundary view's own matrix: the `translate` prop is rounded by the
  helper against the slot grid, whether or not the matrix also holds a
  scale, and nothing else in the matrix rounds. `compose_user` applies the
  translate last, in the slot frame, so this is exact, and a lifted card at
  a static scale following a finger keeps its children rigid. Zero stays
  zero. It is unchanged when the chain through the matrix is not
  axis-aligned, as today: a rotated view does not step.
- A boundary (`repaintBoundary`, view-only) keeps `snapped_own` as it is:
  the absolute origin lands on a pixel, because a recording's interior is
  fixed relative to that origin and has to land on a pixel when replayed.
  Under a running track it does not round.
- `snapped_scroll` rounds the scroll offset with the helper against the box
  grid, under a transition too. On a boundary, whose box grid is whole,
  that equals today's result.

Each raster target keeps its own rounding: glyphs inside the text layer,
then the quad into the window. That is one rounding per target.

### Rounding off

`GridMap` becomes a value with one `unrounded` flag beside the map, read by
`shift`, `snap_box` and the movement helper, so every site above and
`grid::shift` in the text build (a `d-text` included) respect it.
`grid::scale` keeps reading the map.

One thing sets the flag: a running track that moves a box. That is a node
with a track on `X`, `Y`, `Scale`, `ScaleX`, `ScaleY`, `Rotate`, `RotateX`,
`RotateY`, `OriginX`, `OriginY`, `Perspective` or `Layout`, and the flag is
inherited by its subtree. It is read at the node's own matrix as well as
below it, and for `Layout` in the parent's child loop.

Tracks only, not held writes. `any_running` counts both, so this needs a
query of the tracks alone. A held write would drop the rounding when it is
scheduled: the box would step while it is at rest, and its text would stay
soft for the whole delay. With tracks only, the step comes on the frame the
track starts, where the motion hides it. The same holds for an enter under
a stagger: it waits rounded at its `from` value.

Motion is a fact of the transition engine, not an observation of the value.
Nothing watches a value to guess whether it moves: a value written directly
has no track, so it rounds, and to move smoothly it goes through a
transition. `scale_transition_factor` is not the test, as it reads 1 for a
squash of the shorter axis (`scaleX` 1 to 0.5 at `scaleY` 1). The raster's
`scale_observed` stays as it is: it decides when to re-raster, which is a
separate question.

The end needs no wait. `advance_transitions` runs before the frame's paint,
and its settling tick drops the track and writes the exact end value with
Compose damage, so the frame that shows the end is already rounded, with no
extra frame requested.

Motion clears at every raster root, so a snapshot's contents stay on their
texture's grid; `grid::raster` gives that for free. A snapshot's composite
origin still reads the flag of the frame it sits in. Motion does not clear
at a recording root: a recording is no raster, its box grid is not whole
under the track, and rounding against that moving grid would wobble a
recording that re-records in flight. `RecordingCache` stores whether it was
recorded under motion, and a replay with motion clear drops the cache once.
The existing density compare cannot catch this, because a zoom records at
its end density on a mid-motion grid and the density still matches at rest.

A snapshot's own box comes from the frame it sits in, so it is the layout
size under motion and the snapped box at rest, and `SnapshotKey` holds that
box today. Keyed that way, every snapshot under a track would re-raster
when the track starts and again when it ends. `SnapshotCache` keeps the
layout size beside the box it rastered. Under motion the compare is on the
layout size and the quad is drawn at the cached box, so a snapshot made at
rest is reused as it is. At rest the compare is on the snapped box, as
today, so a snapshot made under motion re-rasters once when the track ends
if its box differs, and not at all if the two agree. One re-made under
motion for a content change keeps the box it had cached, so it has nothing
left to do at rest; only one with no cache, or with another layout size,
is made for the layout box. The interior needs no flag: motion is clear
inside the raster either way.

### The override

`pixelSnap={false}` is a prop of every element (`NodeProps`, beside
`label`). It is inherited by the subtree and it is a one-way off: a
`pixelSnap={true}` below it does not turn the rounding back on. It is for
content that must not step when its motion ends, and it costs crispness at
rest: a text quad on a fraction of a pixel is sampled with linear
filtering, and a 1 px edge spreads over two rows. Display type and drawn
art take that; body text and hairlines should not.

It sets the same `unrounded` flag as a running track, from one more source,
so every site above respects it with no code of its own. The parent's child
loop reads the child's prop where it reads the child's motion. The two
sources differ at a raster root: motion clears there, the override does
not. So the override is also one bit in `BuildContext`, OR-ed in down the
walk, and a raster root (a snapshot, a capture) builds its grid with that
bit as the flag. Set above a boundary, the boundary lands on a fraction and
its texture is resampled; set on a `<text>` inside a boundary, the
boundary's own origin still rounds.

A recording made under the flag is not made under motion: the flag does
not clear when a track ends, so nothing re-records. `RecordingCache` stores
the flag it was recorded under, whatever set it, and a replay under a clear
flag drops the cache once; that is the motion rule above, unchanged. A
snapshot under the override always has its layout size as its box, so its
key never changes with a track.

A write to the prop is Paint damage on the node, which brings the walk to
it. A recording or a snapshot below it holds the other rounding, and
notices by itself: both caches store the override bit they were made
under, and one made under the other setting is re-made when the walk
reaches it. Every boundary under the node is reached, because each cached
one on the way down fails the same compare. No walk over the subtree at
the write, and no rule that the prop is set once.

Not part of it: the glyph row and phase snapping inside a text layer,
hinting, and the stroke floor of okf/backlog/border-width-device-pixels.md,
which is a width and not a position.

The prop goes through `packages/core/src/types.d.ts`, the element-level
dispatch in `flux/src/alloy_plugins/properties/mod.rs` (a value that is
not a boolean throws), the read side in `properties/read.rs` so the tree
record shows `pixelSnap: false`, and a field on `Element`.
`packages/core/AGENTS.md` and `docs/guides/animation.md` name it where
they describe motion.

### The raster

`raster_density`: a known end rasters for that end, up or down. Headed down,
while the composite scale is within `LAYER_SCALE_MOTION_BOUND` of the end,
the raster is made at the end scale and magnified in flight, whether or not
a raster exists. The upward case does not change. This reverses the "headed
down, keep and minify" choice of okf/done/text-layer-motion.md, at the cost
of up to 1.18x magnification in the first frames of this pop. The raster
count stays one per zoom: the re-raster moves from the rest to the start.
A text layer, a recording holding text and a snapshot all follow it, as they
share `raster_density`.

`scale_target` resets to 1 at snapshot and capture raster roots. Today it
crosses them, so a text inside a snapshot applies the ancestors' factor on
top of a density that already holds it. The upward case over-rasters; the
downward rule above would under-raster. `for_rest` in `snapshot_node` also
accepts a downward factor, so the texts inside know the raster is for the
rest scale.

## Traps

- Damage needs no change: `last_extent` can stay fractional, because
  `PresentDamage::from_frame` floors, ceils and pads by `DAMAGE_PAD_PX`,
  which holds a rounded edge.
- Cull: the cull rect is fractional while the clip is snapped, so an edge
  row can be up to 0.5 device px apart. Inflate it by that much.
- Static output changes: some nested boxes move by a pixel. That is the
  correction, and pixel tests need re-reading with that in mind. A
  statically translated box can land up to 1 device px from its ideal
  position instead of 0.5: the translation and the box each round once.
- `alloy/examples/pixel_snap.rs` has laid-out rects only, so it passes
  while every detached background is off the grid. The `d-rect` case in
  Done looks like is what guards the box offset.
- `alloy/src/tests/text_motion.rs` pins "headed down, keep and minify". It
  changes with the raster rule.
- A text's underline no longer shares a box offset: it stays fractional
  while the quad rounds.
- `Tree::device_box` matches the paint only for a box with no movement and
  no boundary above it. Narrow its comment. The crop in
  `flux/src/alloy_plugins/record.rs` comes from it, so a cut-out under
  either can be 1 device px off.
- Free-form geometry (paths, ovals, lines, detached shapes with geometry
  of their own) stays unsnapped as it is today, and nothing snaps under a
  rotation or a 3d transform.

## Prior art

Browsers keep layout fractional and snap boxes at paint. WPF's
`TextOptions.TextHintingMode="Animated"` exists for moving text.

## Related

- okf/done/pixel-snapped-paint-boxes.md: the rule this revises.
- okf/done/text-layer-motion.md: the raster hysteresis it builds on.
- okf/backlog/border-width-device-pixels.md: stroke widths on the grid,
  independent of this.
- okf/backlog/pixel-snap-scroll-and-js-motion.md: scroll and values written
  from JS, which stay rounded here unless the subtree takes
  `pixelSnap={false}`.

## Findings

Moved to okf/notes/pixel-snap-motion-findings.md.
