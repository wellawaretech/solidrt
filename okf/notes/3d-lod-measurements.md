---
title: 3D LOD measurements and traps
description: What building level of detail in the spatial core established - the projection's negative proj[5], the half-diagonal measure, and the per-frame cost of re-measuring a thousand groups and a thousand instances under a moving camera.
created: 2026-09-11
---

# 3D LOD measurements and traps

Cut from [3d-lod](../done/3d-lod.md) at completion; the usage contract is
in `packages/3d/AGENTS.md` (Level of detail).

- The projections in `packages/3d/src/math.ts` bake in a y-down clip
  flip, so `proj[5]` is NEGATIVE under both perspective and orthographic.
  Anything reading the vertical focal factor from the matrix takes its
  magnitude. The symptom of forgetting: every measured size negative,
  every LOD level off and every population record the hidden matrix -
  distinct from a missing LOD view, which draws the first level.
- The LOD measure is the sphere around the group's boxes with radius half
  the box diagonal, conservative like the frustum box: an icosahedron of
  radius 3 measures as radius 5.2 and hands over farther than a radius
  estimate suggests.
- Cost, release client on the laptop, `probes/3d-lod-bench.tsx`, the
  camera moving every frame so every group and instance is re-measured
  each flush: 1000 fading node groups (3000 meshes) plus a 1000-instance
  instanced LOD cost about 2.1 ms of scene sync per frame (fade 0: about
  1.9), against 0.83 ms for the same scene as plain groups and a plain
  population (a moving camera re-tests every sink in the cull pass
  regardless) and 0.55 ms for the instanced LOD alone. JS per frame is
  the camera write, 0.02 ms. The sync figure is the core flush: the
  walk, the LOD pass, the cull pass over three times as many sinks, and
  the fade writes (one draw-params command per entry in a band per
  frame). Removing the per-instance level-list clone from the population
  pass changed nothing measurable. A BVH-walked measure is the same
  behavior at a better curve if a profile ever asks, the same note as
  the cull sweep.
- A probe edit that zeroes an instanced capacity throws at startup and
  replaces the running client with the error window; bench constants
  that size buffers are guarded with `Math.max(1, n)`.
