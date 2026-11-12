---
title: Damage rectangles for flush-rendered draw targets, fed by culled draw lists
description: Landed 2026-11-11 - a draw target's or sub-target's pending dirt is full or a few pixel rectangles, one model on both threads; a culled draw list derives the rectangles of a write from its records' cull boxes before and after, through its view, and the target renders only inside them; facts per target in /gpu and a force-full debug switch; the 2d views' matrix made y-down to match their vertex stage. Left open, the window side, app-declared damage and a timing of the built path on a tiler.
created: 2026-11-05
completed: 2026-11-11
---

# Damage rectangles, fed by culled draw lists

Steps 1 and 2 of the item as it was shaped (the measurement, then the
mechanism with feed 1). What is left of it, the window side and feed 2,
is [gpu-target-damage](../backlog/gpu-target-damage.md). The decision and
its contract are in
[draw-order-and-culling](../design/draw-order-and-culling.md), D8.

## The symptom it answered

A culled view of many overlapping sprites, one of them animating: every
frame of the animation paid the whole pass, however small the sprite,
because a target was dirty or not and nothing said where a change landed.
The repo rig (`probes/target-damage-probe.tsx`): 600 overlapping sprites
on a 2000 x 1092 target on the Adreno 610 (SM-T500), a full pass of
16.7 ms with a lighter per-pixel cost and 35.0 ms with a heavier one, and
a press frame 13.9 to 32.2 ms of GPU above an idle one.

## Step 1: measured first

[target-damage-measurements](../notes/target-damage-measurements.md): a
view tiled into sub-targets over culled lists, one sprite pulsing. Pass
time follows the redrawn area, the no-clear pass has no large fixed cost
on the tiler, and an 8 x 8 tile press frame sits 2.6 to 2.9 ms of GPU
above an idle frame where the full pass sits 13.9 to 32.2 ms above it.
Those are the tile rig's figures, measured before the mechanism existed.
They decided the rest was worth building.

## Step 2: what was built

All in alloy (`gpu/damage.rs`, `raster/dirt.rs`, `raster/targets.rs`
`flush_dirty_within`, `gl/target.rs` and `gl/pass.rs` for the groups,
`context/draw_list.rs` for the feed).

- **One dirt model for targets and sub-targets.** An owner's pending dirt
  is full or a set of rectangles in its own pixels (`DirtySet`). Every
  dirtying that names no rectangle is full, full always wins, and
  rectangles union until the owner renders. Each owner renders FULL,
  from its RECTANGLES, or not at all. A parent's own rectangles also
  damage every tile they cross. A parent renders a full pass when it is
  full itself or every tile is; otherwise one partial pass: a group per
  rectangle of its own, then each tile that renders, whole or per
  rectangle. A list on a sub-target and a parent with tiles are the same
  case.
- **A damage group** keeps its owner's viewport (the same picture cut
  down, `iResolution` unchanged), scissors to the rectangle, wipes it
  with the tile-clear draw (color to the clear color, depth to the far
  plane) and redraws the owner's whole entry list under the scissor.
- **Feed 1, the draw list.** At the rebuild point a culled list whose
  view held still turns the slots a write touched into rectangles: each
  slot's cull box as it was at the last point (the mirror now keeps the
  old box of a changed slot) and as it is now, grown by the cell index's
  margin, its eight corners through the list's view into target pixels,
  grown by the box's screen reach (scaled by target over viewport, so an
  oversampled target is right) and by one pixel for the rasterization
  edge, rounded outward. A patch and a rebuild are alike, and so is a
  population change through the instance count: the slots that joined or
  left change only their own boxes. The list's own buffer write carries
  the dirt (`RasterCmd::WriteDrawList`), so it does not count as a full
  dirtying.
- **Facts**, cumulative per target and sub-target
  (`GpuPipelineInfo`): `full_renders`, `damage_renders`,
  `damaged_pixels`; in `/gpu` as `fullRenders`, `damageRenders`,
  `damagedPixels`.
- **The switch**, `Context::set_force_full_renders` (read back with
  `force_full_renders`): while on, every owner with dirt renders in
  full. Control API `/force-full-renders` (GET reads, POST `?active=`),
  the client query `force_full_renders`, the MCP tool
  `set_force_full_renders`, `app.forceFullRenders()` in `@solidrt/test`,
  and `forceFullRenders` at the top of every `/gpu` reply, since it
  outlives a reload. The test host turns it off ahead of every test
  engine. Stale pixels that go away under it name a wrong rectangle.

## The decisions that settled the open questions

- **The tiler cost** (open question 1): the tile rig above found a
  partial pass costing about its share of the full one, and the built
  path timed on the same device afterwards agrees: a damage render of
  one sprite's box costs 0.59 to 0.71 ms where the full pass costs 16.7
  to 35.0 ms
  ([target-damage-measurements](../notes/target-damage-measurements.md)).
- **MSAA targets**: a multisampled pass always renders in full; a tile
  follows its parent's sample count. Damage never draws over
  multisample storage.
- **Depth**: wiped to the far plane per rectangle, as the tile wipe does.
- **Many rectangles**: rectangles merge with every one they meet (overlap
  or touch); past 4 the set collapses to its bounding rectangle; an
  owner more than half covered renders in full; more than 64 touched
  slots at one rebuild point is a bulk change and renders in full. No
  cell grid: a ring-shaped reveal of a few hundred writes is full anyway.
- **The full fallbacks**: the first render, a resize, a context loss, any
  dirtying without rectangles (a params write, an entry or order change,
  a buffer the list does not own), a source that re-rendered, a fragment
  target, a list that does not cull, has no view, is stale (its view,
  viewport, sort key or direction changed) or is capped below its visible
  set, a box that projects without bound (a corner at or behind the eye),
  a pass whose wipe program (the tile-clear draw) failed to compile, the
  switch, and once an owner holding a draw over the slicing floor (65,536
  vertex-instances) that the submission budget has not measured yet: a
  damage group would probe it without recording the measurement
  (`SlicePlan::unmeasured`), so the first change renders in full and
  measures it.
- **The view contract**: the view a culled list projects with must be
  the matrix its vertex stage projects with. The 2d views pushed a y-up
  projection (`proj[5] = -2 / height` in `viewMatrices`) beside y-down
  vertex stages; culling never noticed, since a frustum test is
  symmetric in y, but damage rectangles landed mirrored top to bottom.
  The projection is y-down now (`+2 / height`), and
  `tests/camera.test.ts` pins `proj * view` to the stage's mapping. The
  gated order feed must not drive a culling list (it lags small turns
  and moves); nothing combines them today. 3d feeds only order lists,
  which carry no cells, so damage never runs there.

## Verified

- `alloy/examples/target_damage.rs`: fifteen scenarios on the real GL
  path, each run once as it is and once under the switch, the whole
  readback byte-identical after every step, with the facts and pixels
  inside and outside the damage asserted. The unit tests in
  `alloy/src/tests/target_damage.rs` cover the rectangle maths and the
  list's dirt.
- `packages/2d/tests/target-damage.test.tsx`: through a sprite layer and
  a record layer with `cull`, each beside an unculled twin. At oversample
  1 and 2 a write is one damage render of about the two boxes, its
  changed rows exactly the sprite's, the pixels equal to the twin and to
  the same steps under the switch. Under a rotated, zoomed and pivoted
  camera on a view that is not square, a write before and after a camera
  move renders from rectangles and the move in full; adding and removing
  a sprite are a damage render each; a write renders from rectangles in
  both views of one layer. The unculled twin renders in full throughout,
  and a test leaving the switch on does not reach the next. With the old
  y-up projection every damage test fails.
- On the dev client against `probes/target-damage-probe.tsx` (PLAIN,
  frozen clock, five stepped frames of a press): five damage renders,
  no full ones, about 4,200 damaged pixels each on a 1280 x 720 target,
  2400 vertices per frame (the whole list under the scissor), and
  byte-identical pixels with the switch on, where the same frames are
  five full renders.
- On the SM-T500 (2026-11-12, a release client built from this tree):
  the probe's PLAIN press renders from rectangles at 0.59 to 0.71 ms a
  pass against 16.7 to 35.0 ms forced full, the target's pixels are
  byte-identical with the switch off and on, and the pan, churn, tint
  and reveal scenarios of `probes/record-layer-order-probe.tsx` read as
  on the client it replaced
  ([target-damage-measurements](../notes/target-damage-measurements.md)).
  The wipe and clear bytes are not checked on ANGLE.
