---
title: A splat world larger than one cloud has seams between clouds, no tiling bake, no streaming and no merged far LOD
description: One cloud culls and thins per chunk under the view, but a world made of several clouds blends wrong where they meet because each sorts alone and the scene orders them as wholes; nothing cuts a large capture into cells, nothing loads a cloud's runs on demand, and a far chunk thins to its brightest splats instead of a merged stand-in. The region handlers can already host a cloud per cell; the cross-cloud sort is the decision that needs discussion first.
created: 2026-10-22
---

# A splat world larger than one cloud

## Symptom

okf/design/splats.md lists "No streaming" and "No merged aggregates for
far LOD" as known limits and puts a city-scale capture out of scope.
What one cloud has today (okf/plans/splat-order-and-chunks.md, D13): a
frustum cull and a screen-contribution cut per run in the core's
selection pass, a budget over what passed, positions exact per run box
and local to the mesh node (so the node rides
okf/done/3d-large-world-precision.md). What a world needs beyond that:

- **Seams between clouds.** Each splat mesh is one entry: the core
  sorts its records back to front within it, and the scene's transparent
  entry sort places the entries as wholes. Splats spill past a cell's
  cut, so where two clouds meet their splats interleave in depth but
  blend in entry order, and the order flips as the camera crosses the
  seam. Any world built from more than one cloud shows it, a streamed
  one at every cell boundary.
- **No tiling bake.** `sol tool 3d/splat` turns one capture into one
  `.sol3s`. A capture larger than a device's memory, or larger than one
  cloud's ceiling (`SPLAT_CHUNK_LIMIT`, 65536 runs, which the 16-bit
  chunk index in word 1 bounds), cannot be cut into cells.
- **No streaming.** The region handlers
  (okf/plans/3d-world-residency.md) load, build under the slice budget,
  unload and stand in a proxy for any content, so an app can already put
  a `createSplatMesh` in a cell's `build`; but the cloud uploads whole
  (records, colors, SH labels in row bands), nothing pools its textures,
  and the run, the natural unit to load by, is not loadable on its own.
- **Far LOD thins, it does not merge.** A far run keeps its most
  important splats and drops the tail; the survivors do not grow to cover
  what the rest covered, so a distant cell reads sparse and a proxy for an
  unloaded cell has nothing to be made of.
- **Resident cost is per splat.** About 40 bytes at SH3 under the
  palette: 16 record, 4 color, 8 SH labels, 4 id stream, 8 in the core's
  mirror. 10M resident is about 400 MB, past the low-end Android tier.

## Elsewhere

Spark and PlayCanvas's unified splat mode collect every cloud in the
scene into one sorted list, which is what removes the seam; Hierarchical
3DGS and Octree-GS cut a tree of merged gaussians by screen error for far
LOD; PlayCanvas streams LOD levels of a cloud on demand. Three, Unity and
Godot ship none of this first-party (Three and Unity through third-party
viewers). Check each against its current release when this is shaped
further.

## Done looks like

- **One order across clouds.** Overlapping or adjacent splat meshes in
  a scene blend in depth order across their boundary, with no flip as the
  camera crosses it. The candidate shapes, to decide before building:
  the core merges the selected records of every splat entry in a scene
  into one id stream and one draw (ids then name a cloud as well as a
  record, so the data textures become a shared pool or an array), or
  cells overlap by a margin and cross-fade, which keeps one sort per
  cloud but doubles the splats in the margin. The first changes the
  one-mesh-one-sort model of D2 and D5 and wants a discussion first.
- **A tiling bake.** `sol tool 3d/splat` cuts a capture into cells on a
  grid (or along the k-d partition's upper splits), one `.sol3s` per
  cell in each cell's own frame plus a small index (cell bounds, file,
  splat count), each cell judged by the existing bake report.
- **Cells streamed by the region handlers.** A splat cell content for
  `createRegionGrid` / `createRegion`: load the cell's bytes off the
  frame, upload under the slice budget, weigh the region by its measured
  bytes against the scene limit, pool the data textures across cells.
  Whether a single cloud's runs should page in on their own (the plan's
  "a hand-off that grows") or the cell is the only streaming unit is part
  of the decision above: with one merged order the two converge.
- **Merged aggregates for far LOD.** The bake adds a few coarse
  gaussians per run, merged from its tail, ranked by the ramp below the
  cut so the selection draws them in place of what it thinned; a cell's
  aggregates double as its proxy while unloaded.
- **Measured** on a world of several cells on the Pixel 7 under the
  phone protocol: sort ms with every cell in view, fill from inside a
  cell, the seam judged by a human against one cloud of the same capture.

## Involves

`packages/3d/src/splat-data.ts` and `tools/splat.ts` (the tiling and
the aggregates), `packages/3d/src/splat.ts` and `region.ts` (the cell
content, the pool), `alloy/src/gpu/order.rs` and
`alloy/src/context/order.rs` (a merged order across entries, if chosen),
the scene's transparent entry placement, `probes/splat-mesh-probe.tsx`
(a multi-cell rung), okf/design/splats.md (D2, D5, the limits).

Not in this item: the on-screen fill wall
(okf/backlog/splat-overdraw-fill.md) and floaters away from the capture
path, which are the capture's, not the engine's.
