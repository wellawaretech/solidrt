---
title: The 2d package streams only tiles; sprite populations, priorities and measured chunk weights still wait on the core region lifecycle
description: The 3d residency item put the handler lifecycle (load, sliced build, unload, retain, proxy, measured bytes, streaming sources, priority) in core and a node form in 3d; the tile layer predates it and reports estimates, nothing streams sprite populations per region, and a 2d <RegionGrid> over sprite layers is the same hundred lines 3d's is.
created: 2026-10-20
completed: 2026-10-20
---

# The 2d package streams only tiles

## Symptom

Streaming worlds are one gate plus handlers (okf/done/spatial-residency-gate.md).
The tile layer was the first handler and stays its own: a synchronous
enter and leave around the bake, `fill`/`retain`, its own pool handling.
The 3d item (okf/plans/3d-world-residency.md) then built the handler
LIFECYCLE in core (`createRegion`/`createRegionGrid` in @solidrt/core:
load with abort, a build stepped under the frame slice budget, unload
before teardown, retain, fail-soft, measured weights, priority) and a
node form over it in 3d. Nothing in 2d uses it yet:

- A 2d app streaming SPRITE POPULATIONS per region (a bullet-hell's
  enemies by zone, a city's cars by block) hand-rolls the lifecycle
  against `flux:spatial`, the second app handler the gate note named.
- The tile layer weighs every chunk by its estimate (an empty resident
  chunk weighs a full chunk's bytes) though `setCellWeight` now exists,
  and carries no `priority` (a ground layer before its decoration layer
  when the enter budget is short).
- A 2d streaming source (a scrolling cut-scene's look-ahead, a second
  player on a server) is `createStreamingSource` in 3d only.

## Done looks like

- `createRegionGrid` / `<RegionGrid>` in @solidrt/2d over a sprite
  layer: core's grid lifecycle with a sprite GROUP per cell at the cell's
  corner as the node form (records cell-local), `build` adding sprites
  under it (a generator for many), teardown destroying the group,
  `unload` before it, the measured weight the cell's record bytes, a
  `proxy` optional (a 2d far-ring stand-in is the mip chain's job, so
  most layers pass none). The 2d camera is already a viewer; a 2d grid
  is planar (z unbounded), so the 3d note's "cells behind the eye" is
  the ring around the view here and needs no mention.
- The tile layer reports each chunk's actual bytes (`setCellWeight` at
  acquire and release: an empty chunk weighs nothing) and takes
  `priority` through to `createGrid`.
- `createStreamingSource` reachable from 2d (a point and a radius in the
  plane; the cube view works as is).
- Tests beside tiles-residency.test.tsx; the 2d AGENTS.md Residency
  text pointing at the same contract as 3d's.

## Involves

`packages/2d/src/` (a region.ts beside tiles.ts, components), `tiles.ts`
(weight and priority), `packages/core/src/residency.ts` (consumed as
is), the 2d AGENTS.md.

## Landed

2026-10-20, uncommitted. The scope widened past the "Done looks like"
list on the way in, against what other engines got wrong with streamed
2d worlds:

- `packages/2d/src/region.ts`: `createRegionGrid(layer, ...)` and
  `createRegion(layer, ...)` (the explicit form too, Unreal's streaming
  volume) over core's lifecycle; a group per region with a hidden
  `content` child shown at build end, an optional `proxy`, the measured
  weight the content's record bytes, the teardown snapping;
  `createStreamingSource({ x, y, radius })` over core's (the matrix
  building moved from 3d into `packages/core/src/residency.ts`, 3d
  re-exports it). `components/region.tsx`: `<Region>`, `<RegionGrid>`.
- Every view is a viewer: the view shell (`views.ts`) pushes its camera
  over its viewport to the gate; the tile layer's private push went
  (`viewMatrices` now in `camera.ts`). A sprite layer had pushed nothing.
- `destroySprite`/`destroyGroup` take `{ snap: true }` (free now, no
  exit: Unity's deferred Destroy under an exit animation is how streamed
  objects outlive their cell); `setSpriteParent` and `setGroup`'s
  `parent` take `keepWorld` (Unity's worldPositionStays; Minecraft's
  entity re-homing across chunks), composed from the core's world
  matrices with a snapping write.
- The tile layer: `priority`; measured chunk bytes (`setCellWeight` at
  acquire, release and the enter of an empty chunk; re-pushed on a
  density change), its `maxTexels` limit extended by the record buffers
  of the chunks it holds so it holds exactly those.
- Core: `setLimit`/`setWeight`/`setResidencyLimit`/`setResidencyBudget`
  and `measure` request the frame whose flush re-weighs.
- The spatial core: a shape-less box is a SOLID to overlap and sweep
  (`Query::solid_box_contact` in `alloy/src/spatial/collide.rs`, a
  collider kind beside the capsule and box queries): for a volume whose
  center is inside, the shortest push that carries it clear, over the
  six face normals (through the inverse-transpose, so a sheared box
  under a scaled parent keeps its true normals) and, for a box volume,
  its own three axes; a center outside keeps the exact face-triangle
  test. The 2d layer had promised this ("a volume wholly inside a
  sprite touches it") over a surface-only core; a marquee inside a
  sprite found nothing, and a circle inside near an edge was pushed
  deeper in. Tests: the axis-aligned column, a sheared column with the
  mover, a thin rotated slab taking its own axis.
- Both packages' cell error forwarders fell through to the grid's
  listener, then the console (they had shadowed the grid's).
- Tests: `packages/2d/tests/region.test.tsx` (10), two in
  `tiles-residency.test.tsx`, `spatial_collide.rs` (+4),
  `spatial_residency.rs` (+2). Docs: the 2d
  AGENTS.md Regions section, the group verbs, the tile residency text,
  the component rows, the traps; the 3d AGENTS.md and scene.ts and
  flux-types for the solid-box contract.

## Findings

- An unbounded grid has a cell wherever the view goes: a test that
  moves the camera "far away" expecting an empty grid needs `bounds`.
- The app test harness drains a sliced build in one frame (no wall
  clock), so a cancel mid-build is not testable there; the abort of a
  load in flight is (a promise held open until the leave).
- A limit, weight or budget write to the gate re-tests at its next pass
  and demands no frame on its own, the same trap as a bare `setView`:
  core requests one at every such write.
- The leave band applies to a streaming source too: a source moved one
  cell over keeps the cell it left while within `leave` of it.
- Not done on purpose: a 2d streaming example (3d's `streaming.tsx` is
  the pattern); the tile layer onto core's lifecycle, which would make
  `fill` an async `load` (a chunk generated in an isolate) - its own
  item, [tile-layer-core-lifecycle](../backlog/tile-layer-core-lifecycle.md).
- Three more, found on the way and fixed on the user's call:
  - A 3d record mesh (records as bytes) sits in the picking index by its
    population box, which the solid rule would have made a wall a body
    is pushed out of. `setCollide(node, false)` (alloy `Node::collide`,
    the plugin, flux-types) keeps a box in the index for rays alone;
    `scene.ts` sets it off for record populations and on for sprites.
  - The gate forgot a cell's measured weight at leave, so a cell
    approached again was admitted by the estimate. A grid remembers the
    measured weights of its last REMEMBERED_CELLS (4096) left cells and
    seeds a re-approached cell with it (`residency.rs`).
  - The gate was global across 2d layers and 3d scenes: any 3d camera
    wanted a 2d grid's cells whose coordinates overlapped. Viewers and
    grids are now scoped to a WORLD, the id of a root node: `setView(target,
    view, proj, root?)`, `GridSpec.root`, an explicit region's world its
    top ancestor (a root node's region belongs to none); a viewer or grid
    without a root meets every world. The 2d view shell scopes a view to
    its layer's root (the tile layer makes a root node for its world; a
    record layer's views push no viewer), the 3d scene scopes its camera
    and shadow views to its root, a 3d region grid starts under its
    scene's root, and the streaming sources take the layer or scene:
    `createStreamingSource(layer, { x, y, radius })` /
    `createStreamingSource(scene, { position, radius })`.
  - The pre-existing type error in `packages/3d/tests/gltf.test.ts`
    (`move.curves` after a `fail` that does not narrow) is fixed.
