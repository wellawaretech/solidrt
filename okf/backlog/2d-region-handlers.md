---
title: The 2d package streams only tiles; sprite populations, priorities and measured chunk weights still wait on the core region lifecycle
description: The 3d residency item put the handler lifecycle (load, sliced build, unload, retain, proxy, measured bytes, streaming sources, priority) in core and a node form in 3d; the tile layer predates it and reports estimates, nothing streams sprite populations per region, and a 2d <RegionGrid> over sprite layers is the same hundred lines 3d's is.
created: 2026-10-20
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
