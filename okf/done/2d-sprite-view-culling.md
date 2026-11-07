---
title: A sprite layer draws every sprite, on screen or not
description: A sprite or record layer is one instanced draw over all its records, so its vertex cost follows the size of the world, not what a view shows (about 6 ms of a 41.6 ms drag frame on an Adreno 610 for 30,703 records with about 650 on screen); the fix is view culling per cell of a population's members in the core - members gathered in cell order, cell boxes from the boxes the core already keeps, each view drawing the contiguous instance ranges of the cells its camera meets.
created: 2026-10-14
---

# A sprite layer draws every sprite, on screen or not

Split out of [2d-baked-layers](../done/2d-baked-layers.md) as its part 7.

## Symptom

A sprite layer (`createSpriteLayer`) and a record layer
(`createRecordLayer`) draw as one instanced draw per view over records
`[0, count)`. Every record runs the whole vertex stage: 4 vertices for a
quad atlas, 8 for a hull atlas (`mesh: "hull"`). A sprite off screen is
placed through the camera in full and only then clipped; it shades no
fragments but costs its vertices. So the vertex cost of a frame follows
the size of the world, not what the views show, and a world ten times
larger costs ten times the vertices to show the same screen.

Measured on an Adreno 610 tablet (display scale 1.5, view oversample
1.5, 0.0.86): a record layer of 30,703 hull records, two thirds of them
zero-size slots, about 650 sprites on screen, a scripted 3 s drag:

| | GPU per frame | view pass | outside the pass | vertices |
|---|---|---|---|---|
| hulls | 46.6 ms | 27.1 ms | 19.5 ms | 245,624 |
| quads | 42.9 ms | 31.9 ms | 11.0 ms | 122,812 |

The part outside the pass does not move when only the fragment stage
changes and falls by 8.5 ms when the vertices halve, so it is the vertex
work (a tiler books binning outside the pass; read from the numbers, not
seen directly). A layer of 613 sprites filling the same screen costs
36.5 ms for the same drag.

Since [2d-sprite-vertex-early-out](../done/2d-sprite-vertex-early-out.md)
the hull row reads 41.6 ms (27.0 ms of pass, 14.6 ms outside it) and
the 613-sprite layer 36.0 ms. On the 613-sprite layer about 8.5 ms of
the part outside the pass is there whatever the layer submits, so what
follows the world size is now about 6 ms, and that is the most culling
can win. A zero-size slot now skips its vertex work but still pays its
instance fetch, so the empty slots in cells off screen are part of
those 6 ms.

What culling wins is therefore bounded: it removes the part that
follows the world size, so a large layer costs about what a layer of
its visible sprites costs (here about 36 ms instead of 41.6, still short
of 60 fps). The rest of a drag frame is the view pass and the window
composite, which every camera move pays because a view is a plain target
re-rendered under its camera; that cost is not this item's
(the retained-render side is [gpu-target-damage](../backlog/gpu-target-damage.md)).

The decision of 2026-09-06 in 2d-baked-layers held that an off-screen
record costs one degenerate quad and leaves nothing to win at tens of
thousands of records. The table above shows the premise does not hold
on the tiled-GPU tier.

## Decided

- **Per cell, not per sprite.** Engines cull a tile map per chunk
  (Unity's Tilemap in chunk mode, Godot's rendering quadrants) and an
  instance population per cluster (Unreal's hierarchical instanced
  mesh), not per instance: a sprite is a record, not a draw. A cell
  grid makes the per-frame cost a few box tests per view.
- **The unit is a population's members, not sprite records.** The core
  culls the members of an instanced population by cell, and knows
  nothing of a sprite's record layout. A 3d instanced mesh has the same
  gap ([3d-instance-citizenship](../done/3d-instance-citizenship.md)
  left per-instance culling at a zeroed slot), and it gets the same
  mechanism.
- **Cell boxes from what the core already keeps.** A node layer's
  sprite is a spatial node: its local `bounds`, its `world_box` that the
  flush keeps current, and its `cull_margin` (world units, where the
  outline goes) are already there, and the record sink maps the node to
  its slot. A cell's box is the union of its members' boxes. Reach in
  screen pixels (the hull push, a per-layer constant; the `minScreenPx`
  floor) is a new px margin and px floor on a cull box, converted per
  view at the cell's pixel scale, which `OrderView::scale2` already
  measures for a packed run. A record layer has no member nodes, so it
  declares a record-extent descriptor by field role (position, half
  extents, outline, px floor offsets), the way `OrderKey` names its key
  by offset, and the gather computes its cell boxes from the records.
- **In the core, not in JS.** The first shape of this item queried the
  visible set in JS (`spatial.overlap` over the view rects, node ids to
  slots) and handed the slots back as a mask, on every flush where the
  camera or any pose moved. In a live world that is every frame, O(n)
  JS and a slot array over FFI at exactly the populations this is for.
  The core owns the gather, the key column and the boxes; the cell test
  runs there, and JS only switches it on.
- **Contiguous ranges, not an index stream.** The gather keeps the
  buffer in cell order, so the cells a view meets are contiguous
  instance ranges. A view draws its ranges from the one gathered buffer
  by binding the instance buffers at a byte offset per range (ES 3.0 has
  no base instance). No index stream and no gathered copy per view. The
  box test is `select_runs`'s, and the cells are a `RunTable` (boxes and
  starts) whose ramp is optional; the packed path's thinning stays the
  splats'.
- **The cell lattice is the residency grid's.** Cells index the lattice
  of a residency `GridSpec` (origin, cell size, placing node, world),
  the one 2d tile chunks already use and
  [2d-sprite-layer-view-relative-records](../backlog/2d-sprite-layer-view-relative-records.md)
  wants to share, not a second grid type.
- **Order.** A world-wide order and a 2d grid do not mix: in cell order
  a cell's sprites all draw before the next cell's, whatever their keys.
  - Under `orderBy: "y"` the buffer is already in y order, so any y
    interval is contiguous and no cell key is needed: the cells are
    bands of the lattice in y, and a view draws one range. The order is
    exact; the culling is on y alone.
  - Without `orderBy` the cells are a 2d grid in row-major order, and a
    view draws one range per cell row it meets. Cell order replaces slot
    order across cells: two sprites that overlap across a cell boundary
    can swap, the trade Unity's chunk mode makes. The docs say so.
  - Any other world-wide order (`renderOrder`, a record layer's
    `{ field }`) together with `cells` throws at creation.
- **The view comes from the order feed.** The view's entry is a draw
  sink of the layer's node with `order_feed` on, so the core writes its
  model-to-clip matrix; views.ts computes nothing per camera move. A
  record layer gets a node for this, as 3d's records form has one node
  over a JS-written buffer. The feed's re-selection gate for cells is a
  view edge crossing a cell boundary, not `ORDER_VIEW_MOVE_FRACTION`,
  whose eye distance means nothing under a 2d orthographic pan.

## Done looks like

- **The option.** `createSpriteLayer(..., { cells: { size } })` and the
  same on `createRecordLayer`, `size` in layer pixels (the band height
  under `orderBy: "y"`). Off by default: a layer that fits its view
  gains nothing and pays the gather, and a layer without `orderBy` has
  no gather today.
- **Cell order.** A cell key beside `OrderKey::Field` and `Projected`
  reads a record's position and keys it by its lattice cell. The gather
  keeps the GPU buffer in cell order while the app's slot i stays sprite
  i, as `orderBy` does today; a sprite that moves into another cell
  moves in the buffer as a record whose key changed does now. The runs
  start where the sorted key changes.
- **Cell boxes.** A node layer's from its member nodes' world boxes and
  cull margins; a record layer's from its extent descriptor. The px
  margin and floor apply per view at the cell's pixel scale.
- **Ranges.** `DrawRange` gets a first instance and an entry draws a
  list of ranges; the raster binds the instance buffers at the range's
  offset. A camera move reselects without any record upload.
- **2d views as spatial views.** A view sets its target's spatial view
  instead of a separate viewer key, so the order feed and the residency
  gate read the one view. The 2d camera's matrices are those
  `pushViewer` builds now.
- **Picking and queries unchanged.** The selection is draw-only.
- **Measured.** On the tiled-GPU tier, the drag above on the same board
  before and after: the instances submitted (from `/gpu`), the part
  outside the pass, and the gather and selection cost per camera move on
  the CPU. The docs give the record count where the option starts to
  pay.

## Involves

`alloy/src/gpu/order.rs` (the cell key, runs from key changes, the
optional ramp, cell boxes from an extent descriptor),
`alloy/src/context/order.rs` and `alloy/src/gpu/vocab.rs` (the range
list, the first instance), the raster's instance binding at an offset,
`alloy/src/spatial/mod.rs` and `cull.rs` (cell boxes from member boxes,
the px margin and floor, the cell gate on the order feed),
`alloy/src/spatial/residency.rs` (the lattice shared),
`flux/src/alloy_plugins/gpu.rs` and flux-types (the key, the descriptor,
the ranges), `packages/2d/src/layer.ts`, `records.ts` and `views.ts`
(the option, the layer node and its sink, the views as spatial views,
the order rule), `packages/2d/agents/views.md`.

Related: [2d-sprite-layer-view-relative-records](../backlog/2d-sprite-layer-view-relative-records.md)
splits a node layer's records into per-cell buffers for precision, one
entry per resident cell per view; each such entry would then be culled
by the core's frustum gate on its own. It applies to node layers under a
region grid only, so it does not replace this item for record layers;
where both land, they share the cell lattice. The 3d instanced mesh
uses the same core switch; its component's option is its own change.
[2d-sprite-vertex-early-out](../done/2d-sprite-vertex-early-out.md) made the
records that are still submitted cheaper: zero-size slots inside visible
cells skip the vertex work, and every drawn hull sprite reads three mesh
points a vertex instead of eight. Sprites just off screen inside a
visible cell stay the clipper's.

## Findings

Built and tested (2026-11-06); the measurement on the tiled-GPU tier is
the open line. Where the build differs from the decided shape:

- **Spans, not `DrawRange`.** `DrawRange` stays the app's range; a draw
  entry carries a separate list of instance spans (`SetDrawSpans`), cut
  to the range's count at issue. The raster binds the instance pointers
  at each span's first instance (`point_instances`, the slicer's own),
  and slicing runs inside a span.
- **No px margin.** A hull layer's fragments discard outside the frame
  rect, and an outline draws inside the quad, so a sprite's pixels never
  leave its screen-size-clamped rect. A cull box carries one px term,
  the `minScreenPx` floor's reach (`set_cull_px` on a node,
  `minScreenPx` in a record extent). The per-view scale is the entry's
  `orderViewport` (view pixels), not the target's texels, which an
  oversample would make too large.
- **The feed gate.** The cell feed (`orderFeed: "cells"`) writes on
  every change of the matrix; the registry re-selects (a box test per
  cell) and sends spans only when they changed. A lattice-edge gate
  does not hold for union boxes, which overhang the lattice.
- **Cell boxes for a node layer** come from the spatial record sinks:
  `set_instance_boxes(buffer)` stages each slot's box in the record frame
  beside its pose and hands it to the gather with the records. A
  collapsed pose (a zero axis) has the empty box, as do hidden and
  unbound slots: all sort past every cell and no view submits them.
- **The frame node.** A view's entry is a sink of a frame node at the
  layer's render origin (views.ts), and the view's spatial view carries
  the same origin, so the fed matrix maps the origin-relative records.
- **Re-homing.** A retaining order re-attached over a surviving mirror
  now seeds from it, and views.ts removes the disposed view's ordered
  entry before re-homing. Before, the re-home threw ("already ordered"),
  which also hit a multi-view `orderBy` layer.

## Measured

The same Adreno 610 tablet (Galaxy Tab A7, schedutil, thermal status 0,
battery 20 C) and the same 30,703-record hull board, on this build: the
board's record layer with and without `cells`, three runs of a 3 s
touch drag each after a fresh reload, read from `/stats` over the drag
window and `/gpu`. The runs agreed within 0.2 ms.

| | GPU per frame | view pass | outside the pass | instances at the end | vertices per frame | frames in 3 s |
|---|---|---|---|---|---|---|
| no cells | 41.7 ms | 27.1 ms | 14.6 ms | 30,703 | 245,624 | 71 |
| cells 256 | 36.1 ms | 27.5 ms | 8.6 ms | 683 | 4,530 | 83 |
| cells 512 | 36.0 ms | 27.4 ms | 8.6 ms | 965 | 7,130 | 83 |
| cells 1024 | 36.0 ms | 27.3 ms | 8.7 ms | 1,921 | 10,480 | 83 |

- The part that followed the world size is gone: outside the pass falls
  to the 8.6 ms the 613-sprite board pays whatever it submits, as
  predicted. That is about 0.19 microseconds of GPU per off-screen hull
  record per frame. The cell size does not matter on this board.
- A camera move costs no measurable CPU: the JS-thread frame time over
  the drag is 0.08 ms at p50 with and without cells (its worst frame
  0.6 ms against 0.4).
- A frame that writes records pays the gather: over the first reveal's
  animation the JS thread takes 14 to 15 ms at p50 (p95 21 ms) with
  cells, against 1 ms without. Plain `orderBy: "y"` already takes 7 to
  14 ms at p50 (p95 19 ms) there, so most of it is the whole-prefix
  publish and gather the two share, and the cells add 2 to 4 ms (the
  extents and the runs). Open as
  [per-view-draw-lists](../done/per-view-draw-lists.md).
