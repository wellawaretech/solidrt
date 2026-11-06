---
title: A sprite layer draws every sprite, on screen or not
description: A sprite or record layer is one instanced draw over all its records, so its vertex cost follows the size of the world, not what a view shows (19.5 ms of a 46.6 ms drag frame on an Adreno 610 for 30,703 records with about 650 on screen); the fix is view culling per cell in the core - records gathered in cell order, each view drawing only the cells its camera meets - on the selection path packed populations already use.
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
1.5, 0.0.86): a record layer of 30,703 hull records, about 650 sprites on
screen, a scripted 3 s drag:

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
can win.

What culling wins is therefore bounded: it removes the part that
follows the world size, so a large layer costs about what a layer of
its visible sprites costs (here about 36 ms instead of 41.6, still short
of 60 fps). The rest of a drag frame is the view pass and the window
composite, which every camera move pays because a view is a plain target
re-rendered under its camera; that cost is not this item's
(the retained-render side is [gpu-target-damage](gpu-target-damage.md)).

## Decided

- **Per cell, not per sprite.** Engines cull a tile map per chunk
  (Unity's Tilemap in chunk mode, Godot's rendering quadrants), not per
  instance: a sprite is a record, not a draw. A per-sprite visibility
  set is what the decision of 2026-09-06 ruled out; a cell grid makes
  the per-frame cost a few box tests per view.
- **In the core, not in JS.** The first shape of this item queried the
  visible set in JS (`spatial.overlap` over the view rects, node ids to
  slots) and handed the slots back as a mask, on every flush where the
  camera or any pose moved. In a live world that is every frame, O(n)
  JS and a slot array over FFI at exactly the populations this is for.
  The core already owns the gather and the key column; the cell test
  runs there, and JS only switches it on.
- **On the packed selection path.** alloy already selects records under
  a view inside an ordered entry: a packed population (splats) carries a
  run table, a run wholly outside the entry's `orderView` is skipped,
  and the entry draws only what the selection kept (`select_runs` in
  `alloy/src/gpu/order.rs`, `drawn_range` in `alloy/src/context/order.rs`).
  Cells are runs whose boxes the core keeps itself instead of the app
  passing a table. No second mechanism next to it.
- **Both layer kinds.** The node layer and the record layer share the
  instanced draw, so the option and its rules are the same for both.
  The key comes from the pose buffer on the node layer and from the
  record's position on the record layer.

## Done looks like

- **The option.** `createSpriteLayer(..., { cells: { size } })` and the
  same on `createRecordLayer`, `size` in layer pixels. Off by default: a
  layer that fits its view gains nothing and pays the gather.
- **Cell order.** A new order key (beside `OrderKey::Field` and
  `Projected`) reads a record's x and y and keys it by the grid cell they
  fall in. The gather keeps the GPU buffer in cell order while the app's
  slot i stays sprite i, as `orderBy` does today. A sprite that moves
  into another cell moves in the buffer as a record whose key changed
  does now.
- **Cell bounds.** The core keeps, per cell, the box of its records'
  positions grown by the largest reach among them (half the frame's
  diagonal times its scale, plus the hull push and the outline). A reach
  that depends on zoom (`minScreenPx`) is taken at the view's scale.
- **Selection per view.** Each view feeds its camera to its entry as the
  `orderView` (the clip matrix views.ts already builds for the residency
  gate in `pushViewer`). A cell whose box lies wholly outside it is not
  drawn; the entry's instance count is what the selection kept. A camera
  move reselects without any record upload from JS. Open point: the
  packed path materializes an index stream (4 bytes a record), a sprite
  layer gathers whole records, so a view with its own selection needs its
  own gathered copy; measure that against one selection over the union
  of the views before choosing.
- **Order.** Cells change the draw order across cells. That cannot show
  for sprites that do not overlap (a board, most maps). Under `orderBy:
  "y"` the cells are horizontal bands drawn top to bottom and sorted by
  y inside, which is the same order as today. Any other world-wide order
  (`renderOrder`, a record layer's `{ field }`) together with `cells`
  throws at creation.
- **Picking and queries unchanged.** The selection is draw-only.
- **Measured.** On the tiled-GPU tier, the drag above before and after:
  the part outside the pass falls to about what a layer of the visible
  sprites costs, and the gather and selection cost per camera move on the
  CPU is reported beside it. The docs give the record count where the
  option starts to pay.

## Involves

`alloy/src/gpu/order.rs` (the cell key, cell boxes as runs, the
selection), `alloy/src/context/order.rs` (per-entry selection state, the
drawn range), `flux/src/alloy_plugins/gpu.rs` and flux-types (the key in
the order setup), `packages/2d/src/layer.ts`, `records.ts` and
`views.ts` (the option, feeding `orderView` per view, the order rule),
`packages/2d/agents/views.md`.

Related: [2d-sprite-layer-chunked-records](2d-sprite-layer-chunked-records.md)
splits a node layer's records into per-cell buffers for precision, one
entry per resident cell per view; each such entry would then be culled
by the core's frustum gate on its own. It applies to node layers under a
region grid only, so it does not replace this item for record layers;
where both land, they share the cell grid.
[2d-sprite-vertex-early-out](../done/2d-sprite-vertex-early-out.md) made the
records that are still submitted cheaper: zero-size slots inside visible
cells skip the vertex work, and every drawn hull sprite reads three mesh
points a vertex instead of eight. Sprites just off screen inside a
visible cell stay the clipper's.
