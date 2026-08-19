---
title: Geometry as data - transform, merge, public bounds
description: The generators build geometry and nothing can move or combine it, so a static scene authored as data has to become one Mesh node per part; transformGeometry and mergeGeometries are pure array math with no runtime dependency, and geometryBounds plus rayBoxDistance already exist unexported.
created: 2026-08-17
---

# Geometry as data - transform, merge, public bounds

Symptom: a static scene of N parts costs N `Mesh` nodes, N draw entries and
N `uModel` writes on the first sync, because there is no way to bake a part's
placement into its vertices and concatenate the results. `@solidrt/3d`
generates geometry (`box`, `sphere`, `cylinder`, the profile/sweep kit) and
then only ever consumes it whole. With a transform-and-merge pass the same
scene bakes into one mesh per material and only the things that actually move
keep a draw entry of their own.

That is not a micro-optimisation. It is the difference between a scene
authored as data and a scene authored as a node graph, and on an interpreted
engine the per-frame walk is the cost that matters. Three's equivalent is
`Geometry.applyMatrix4` plus `BufferGeometryUtils.mergeGeometries`; both are
pure array math over the interleave, no engine call, no GPU state.

## The surface

```ts
transformGeometry(geometry, { position?, rotation?, quaternion?, scale? }): Geometry
mergeGeometries(parts: Geometry[], label?): Geometry
geometryBounds(geometry): Float32Array   // exists at geometry.ts:76, not exported
rayBoxDistance(...)                      // exists at bvh.ts:45, not exported
```

## Notes for the implementation

- Normals need the inverse transpose; `normalMatrix` is already exported from
  `./math.ts`.
- Merge has to widen the index array to `Uint32Array` past 64k vertices, the
  same rule `packIndices` already encodes.
- Merge must reject mixed layouts rather than dropping a channel: the strides
  disagree, so a silently merged result renders as garbage, not as a mesh
  missing its colours.
- `fillColors` was written for exactly this consumer. Its doc comment already
  describes "a merging builder baking colors over its packed buffer" and "a
  packer that bakes transforms while writing", so the in-place half of the
  colour path is done and only the packer is missing.

## Adjacent, smaller

The withColors double-generation cost is not part of this item; it is its own
backlog entry: [3d-colored-generators](../backlog/3d-colored-generators.md).

## Why the ray helper rides along

`scene.raycast` is volume-tier and only tests meshes, so an app that keeps its
own box list (collision, triggers, a projected ground marker) cannot ray-test
it without creating meshes it does not want to draw. `rayBoxDistance` is
already written and already exercised by the BVH descent; exporting it is the
whole change. Same shape as `geometryBounds`: the code exists, apps cannot
reach it. Triangle-accurate picking stays with roadmap item 4 and is
unaffected.

Ranked as item 20 in [3d-roadmap](../notes/3d-roadmap.md). Demand recorded
2026-08-17.

## Shipped 2026-08-19

`transformGeometry(geometry, transform, label?)` and
`mergeGeometries(parts, label?)` in `packages/3d/src/geometry.ts`;
`geometryBounds` and `rayBoxDistance` exported from the package root. The
transform takes the setTransform shape (`TransformUpdate`, shared with setTransform via math.ts: position,
rotation XOR quaternion, scale as number or Vec3) rather than a Mat4; a
matrix form can widen it if a consumer appears. To keep geometry.ts a pure
module (so the new `checks/geometry-check.ts` rig runs headless on flux),
the two GPU-touching functions (`geometryBuffers`, `disposeGeometry`) moved
to `geometry-gpu.ts`. Documented in `packages/3d/AGENTS.md`.

