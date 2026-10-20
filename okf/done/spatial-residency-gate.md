---
title: A residency gate in the spatial core, one for 2d chunks and 3d regions
description: Culling and LOD gate drawing, not memory; the residency decision (which regions of a world should exist right now, per viewer, with hysteresis and pacing) is the same primitive for a 2d tile world's chunks and a 3d open world's regions, so it is built once in the spatial core and both packages are handlers over it
created: 2026-10-14
---

# A residency gate in the spatial core

The shared contract behind [2d-baked-layers](2d-baked-layers.md) stage B2
and [3d-world-residency](../backlog/3d-world-residency.md). Both items need the same
decision: given every viewer's view volume, which regions of a world
should be resident, told only when that changes, without thrash at a
border and without a burst of creates on a fast camera. Unity, Godot and
Three all leave that decision to the app (scenes, addons, hand-rolled
distance checks in the frame callback); Unreal's World Partition and
Minecraft's chunk columns show its grid form streaming 3d content. The
core already holds every viewer's frustum and runs culling and LOD as
per-viewer spatial tests in Rust with no per-frame JS for a still camera
(`alloy/src/spatial/`, `cull_pass`, `lod_pass`); residency is the third
gate beside them, and the one that gates MEMORY instead of drawing.

Split that keeps 2d and 3d symmetric: the GATE decides, paces and is the
one place a budget lives; a HANDLER acts. The tile layer is a built-in
handler because its region content is engine-owned (a baked chunk and
its records); a 3d region's handler is the app's because its content is
the app's models; a 2d app streaming sprite populations per region is an
app handler too. The asymmetry is in who owns the content, never in the
mechanism.

## Viewers

A viewer is what `setView(key, view, proj)` already describes: a key and
two matrices, from which the core derives the frustum (`cull.rs`) and the
eye (`LodView`). The 3d scene, its views and its shadow tiles push theirs
on every camera write (packages/3d/src/scene.ts `setView`), keyed by the
target's texture id. A 2d camera is an orthographic view and projection
over its viewport, so a tile layer's composite pushes the same two
matrices on camera change; it has no target, so its key comes from
`allocViewKey()`, which hands out keys in a range the core reserves above
any texture id (documented, so a key never collides with a target). A
viewer that stops (`setView(key, null)`) wants nothing; its regions leave
if no other viewer wants them. Nothing is read from the rendertree: the
viewer pushes, as every camera already does, and a still camera costs
nothing.

## Regions, two forms

**Explicit**: `setRegion(node, { enter, leave } | null)` marks an
existing node (its `setBounds` box, carried through its world matrix as
the cull box is) as a region. A level section, a building interior, a 2d
platformer's room. It moves with its parent like any node; destroying
the node ends the region (a leave fires if it was wanted).

**Grid**: `createGrid({ origin, cell, enter, leave, bounds?, wrap?,
label? })` returns a grid id. `cell` is `[w, h, d]` with 0 meaning
unbounded along that axis: a 2d tile world's chunk grid is `[chunkW,
chunkH, 0]` in its layer's plane, a Minecraft-style column grid is `[32,
0, 32]` over XZ, a voxel world `[32, 32, 32]`. `bounds` is an optional
cell-index box (cells outside never exist), `wrap` an optional period in
cells per axis (a torus: indices modulo the period, the canonical cell
reported once however many images the viewer sees; the handler's
composite places the images, it knows the view rect). A grid needs no
BVH: the cells under a view volume are floor divisions over the volume's
AABB, then a per-cell frustum test. `destroyGrid(id)` leaves every
wanted cell. Cells have no state in the core beyond wanted or not; a cell
that was never approached costs nothing, which is what an unbounded or
procedural world needs, since its cells do not exist until approached.

## The decision

Per viewer, a region is WANTED while its box is within `enter` of the
view volume: the frustum's planes pushed out by `enter`, the margin
`cull.rs` already supports on normalized planes. It stays wanted until
the box is beyond `leave` of the volume (`leave >= enter`, validated),
the two-radius hysteresis that keeps a camera on a border from
thrashing, the same one-sided band `LOD_HYSTERESIS` gives LOD switches.
A region is wanted overall if ANY viewer wants it: a minimap beside the
main view, a shadow camera, a split screen. The pass runs at flush, after
`cull_pass`, only for viewers whose view changed and regions that moved
(the dirty lists culling keeps); a still world costs nothing.

The viewer's view volume is bounded along its forward axis by `leave` for
the grid enumeration (an infinite far plane would enumerate infinite
cells); an explicit region is tested against the frustum as culling tests
a box, so a far region in view is wanted, which is what a load distance
on the region expresses when it should not be.

## Pacing

Changes are reported, never polled: one `spatialRegionEnter` and one
`spatialRegionLeave` event per change, payload `{ node }` for an
explicit region and `{ grid, cell: [i, j, k] }` for a grid cell,
delivered in the frame's event slot with the other spatial events
(`deliver_motion`, after `frameStart`, before the frame's callbacks), so
a handler's creates land in the same frame.

Leaves are immediate. Enters are PACED: `setResidencyBudget(n)` caps the
enters delivered per frame scene-wide, nearest to a viewer's eye first
(a frame-work budget: two tile layers and a 3d grid beside them spend it
together, never each their own). A region still pending when the camera
turns away never enters (no event, nothing to cancel); the pending set is
re-sorted each frame, so a fast camera streams what it is heading for.
Default budget is a named constant, generous on desktop; an app tunes it
once. The handler's own readiness (a chunk baked, a model parsed) is the
handler's state, not the gate's: the gate knows wanted or not wanted,
and a handler that is still loading when the leave arrives cancels its
own work.

## The pool

Resources freed on a leave are taken again on the next enter instead of
destroyed and created: chunk allocation measured 1-2 ms on a desktop GPU
([2d-tile-bulk-writes](../done/2d-tile-bulk-writes.md)), a region of
models is many of those. The pool is a JS free list in
`@solidrt/core/gpu`, over GPU ids, since that is where ids live:
`createResourcePool({ cap })` with `take(cls)`, `give(cls, id)` and
`drain()`, `cls` a string class the consumer builds (a shader target's
pipeline, since `setTargetSize` resizes id-stable; a buffer's byte
size), LRU destroy beyond the cap, drained with its owner. The tile
layer's handler takes chunk buffers and targets from it, a 3d region
handler its geometry; neither writes a free list of its own.

## Memory legible

`get_gpu_resources` groups a consumer's resources under its label
(creates already carry labels) with a per-label total, so a world's
resident footprint is a number before and after eviction. `/stats` gains
the gate's counters: regions wanted, enters and leaves delivered this
window, enters pending, pool hits and misses.

## Not in this item

- **Budgeted creates**: spreading one region's many GPU creates over
  frames (the 3d note's "loading that does not hitch") is the handler's;
  the gate's budget bounds how many regions START per frame. Built as
  core's frame slice queue in the 3d handler item.
- **HLOD**: whether a region whose far level is a merged proxy is an LOD
  level that loads or its own gate ([3d-world-residency](../backlog/3d-world-residency.md)).
- **Region content**: what a handler puts in a region is its own; the
  tile layer's bake is [2d-baked-layers](2d-baked-layers.md), a 3d
  region's model loading the 3d note.

## Involves

`alloy/src/spatial/residency.rs` (regions, grids, the pass; `Spatial`
gains the region fields and `residency_pass` in `flush` after
`cull_pass`), `alloy/src/tests/spatial_residency.rs`,
`flux/src/alloy_plugins/spatial.rs` (`allocViewKey`, `setRegion`,
`createGrid`, `destroyGrid`, `setResidencyBudget`, the two events in
`deliver_motion`), `packages/core/src/gpu.ts` (the pool), flux-types for
the new surface, `/stats` and `get_gpu_resources` grouping, the Culling
and Level of detail sections of `packages/3d/AGENTS.md` gaining a
Residency sibling, and the two consumer items.

## Findings

Landed 2026-10-14 in four steps (the gate, the pool, the tile layer as
first handler, then memory legible and bounded).

- **Limits are weighted and urgency-ordered, not a cell cap.** The first
  shape for `maxTexels` was a per-grid cell count; against Unreal's
  streaming pool (one budget, priority eviction) it became a per-grid
  `weight` (the handler's cost of one cell, any unit) under the grid's
  own `limit` and a scene-wide `setResidencyLimit`, with eviction by
  URGENCY, the distance to the nearest wanting eye in the grid's own
  `leave`s, so grids in different units share one scene limit. The tile
  layer's `maxTexels` is the grid's limit with the chunk's texels as
  weight, re-pushed on `setOversample`.
- **Victims are a minimal set after a feasibility check.** The first
  eviction scan took every less-urgent resident in order until the
  candidate fit; a test caught it evicting a light cell uselessly before
  a heavy one and re-admitting it next pass. Now nothing is evicted
  unless the evictable weight covers the need, and once the least-urgent
  set is picked the unnecessary members are dropped most-urgent-first.
  An evicted cell goes back to pending and may re-enter when the slack
  fits it; it then stays, since it fits without an eviction.
- **Stalled is a third state in effect.** A cell a limit holds back is
  wanted and pending but demands no frame and no pass (`pending >
  stalled` is the gate's demand), or a still camera at a full budget
  would spin frames forever. Recounted by every pass.
- **A limit counts cells, not content** was the first shape: a resident
  chunk without cells held no texture but weighed a chunk. The 3d
  handler item (okf/plans/3d-world-residency.md) added the measured
  weight per region (`setCellWeight`, `setRegionWeight`), explicit
  regions under the limits with a weight of their own, and a priority
  ahead of urgency; the tile layer still reports its estimate per
  chunk (an empty chunk weighs a chunk), now in bytes.
- **Memory legible**: `/gpu` `groups` by exact label (the tile layer
  labels every chunk resource with the layer's label), `/stats` reads the
  gate live (`regionsWanted/Resident/Pending/Stalled`, cumulative
  `regionEnters/Leaves` with the window's pair, `residentWeight`), and
  the pools reach `/stats` through `sol:dev` `registerStat`, the
  user-counter registry Godot (`add_custom_monitor`) and Unity
  (ProfilerCounter) have, under `app`. The pool module stays pure (its
  test runs on bare flux); core's gpu module registers its counters.
- **Not built**: the sprite layer's record compaction (part 7 of the 2d
  note) - its "one query per camera move" premise fails for moving
  sprites; shaped in okf/backlog/2d-sprite-record-compaction.md.
