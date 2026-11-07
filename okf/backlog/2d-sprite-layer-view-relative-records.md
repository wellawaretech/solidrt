---
title: Views of one sprite layer far apart from each other see f32 steps in the pose records
description: A sprite layer's pose buffer is one f32 record per sprite relative to ONE render origin, snapped near the primary view's camera; a second view more than about 10^5 px from it (a split screen across a continent) reads records quantized to whole pixels; the fix is a rebase in each view's draw-list gather from f64 positions the record sink stages beside the records, each view relative to its own target's origin (camera-relative rendering), not records chunked per region cell.
created: 2026-10-20
---

# Views of one sprite layer far apart from each other see f32 steps

## Symptom

The spatial arena keeps sprite positions in f64 and the sprite layer
uploads its pose records relative to a render origin
(okf/done/3d-large-world-precision.md, `flux:spatial
setInstanceOrigin`), so a layer whose camera is sixteen million pixels
out renders exactly like one at zero. The origin is the PRIMARY view's
(the first live view, views.ts): one buffer holds one record per
sprite, and a record is relative to one point. A second view of the
same layer whose camera is more than about 10^5 px from the primary's
sees the records around its own camera at f32's spacing there - whole
pixels at 10^7 px - and sprites step as they move. Two views that far
apart over one world is a real layout (a split screen over a large map,
a minimap zoomed in on a distant marker), and today the answer is a
layer per view, which doubles the population's writes.

A second cost has the same cause: a re-snap of the one origin restages
every shown record on the buffer (`set_instance_origin`), so a pan that
crosses an origin cell rewrites the whole population, though each view
draws only what it sees.

The tile layer has no such limit: its records are chunk-local
(okf/done/2d-baked-layers.md stage B2) and its view stage subtracts the
camera's chunk index before scaling, so every chunk is exact under any
view. The 3d side has none either: an entry's `uModel` is per target, so
each target's origin applies to it.

## Decided (2026-11-07)

A sprite layer culls by default, and a culled population draws per view
from a draw list
([draw-order-and-culling](../design/draw-order-and-culling.md), D2 to
D5): each view already gathers its visible records into buffers of its
own. That gather is where the rebase goes, so no single origin has to
serve two views: Unity HDRP's camera-relative rendering, done on the
gather instead of in the shader.

Rejected: the records of a region cell in a buffer of that cell,
relative to the cell's corner, drawn as one entry per resident cell per
view (the tile layer's chunking applied to populations). It moves a
node's record between buffers when its sprite changes cell, multiplies
entries and binds by the resident cells, and ties precision to
`<RegionGrid>`; the per-view gather has none of that and also removes
the whole-population restage.

## Done looks like

Two views of one sprite layer, their cameras 10^7 px apart, both render
their sprites at sub-pixel precision; the population costs one write per
sprite move, and a view's origin re-snap costs a rebuild of that view's
list only.

- **The sink stages f64 positions.** A record sink on a buffer a draw
  list reads stages each record's position in f64 into a column of the
  list mirror beside the f32 records, as it stages the cull boxes
  (`write_boxes`); the projection says which floats are the position.
- **The gather rebases.** A list's gather writes the position floats of
  each drawn record as `f32(position - origin)`, with `origin` its
  view's target origin (`set_origin`), and the bytes it copies today for
  the rest. A change of the target's origin rebuilds that list (its
  visible records), nothing else.
- **Each view snaps its own origin.** views.ts loses the primary view
  and the layer-wide origin: each view snaps its target's origin from
  its own camera, and its `uCamera` is relative to it.
  `setInstanceOrigin` goes from the layer if nothing else uses it.
- **The limit, documented.** A `cull: false` sprite layer draws its
  buffer directly and keeps one origin (the first view's); views far
  apart over it want `cull` on. A record layer's records are the app's
  own f32 world records, as written; the rebase does not apply.

## Involves

`alloy/src/spatial/mod.rs` (the sink's f64 position staging, the origin
restage limited to directly drawn buffers), `alloy/src/context/order.rs`
and `context/draw_list.rs` (the position column in the mirror, the
rebase in `upload_list` and `patch_list`, the rebuild on an origin
change), `alloy/src/gpu/draw_list.rs` (`gather_list` with the rebase),
`packages/2d/src/views.ts` (an origin per view), `camera.ts` (the
camera params per view origin), `packages/2d/AGENTS.md` (The camera)
and `packages/2d/agents/views.md` (Views). Tests: two views 10^7 px
apart render a sprite's sub-pixel move in both; a re-snap of one view
rebuilds its list only.
