---
title: A geometry updated in place keeps picking against its old positions
description: updateVertices re-uploads a stream and drops the cached bounds, but the picking shape the spatial core built at first draw keeps the positions it was built from, so a deforming mesh picks where it used to be until it is re-attached.
created: 2026-09-11
completed: 2026-09-11
---

# A geometry updated in place keeps picking against its old positions

## Symptom

`updateVertices(geometry, ...)` (packages/3d/src/geometry-gpu.ts)
rewrites a stream's GPU buffer in place and clears `_bounds`, but the
triangle picking shape (`createShape` in flux:spatial, one per geometry,
referenced by every mesh node drawing it) is immutable once created and
holds a copy of the positions from the first acquire. A cloth or a wave
surface therefore picks against its rest pose. Documented as the stage
2 rule in okf/done/3d-vertex-data-model.md: re-attach a deforming
geometry that must pick.

## Comparison

Three's raycast reads the live attribute arrays, so it follows every
update. Unity's and Godot's collision shapes never follow a mesh update;
the app rebuilds the collider. SolidRT's shape is closer to a collider,
but a re-attach for a per-frame update is not a workable rebuild.

## Done looks like

A stream-0 update on a triangle geometry that is on the GPU refreshes
its shape: either an in-place `updateShape(shape, vertices, ...)` in
flux:spatial that rewrites the positions the shape holds, or a rebuild
that swaps the shape id on every node referencing it.

## What it involves

A spatial-core verb (`flux:spatial` and its Rust owner), the entry
bookkeeping in geometry-gpu.ts (the shape is per geometry; the nodes
that set it live in scene.ts), and a rig case: pick after an update
lands on the moved triangle.

## Landed (2026-09-11)

The shape was not the only stale thing. The core node's box was pushed
once at attach from a JS scan of the positions and never again, and the
broadphase and culling read that box: a wave surface whose rest pose is
a flat plane would have picked nothing after a shape fix alone, and
lines and points (box-only) had the same gap. Two owners of one dataset,
kept in sync by hand, was the fault, so the fix moved the box to the
side that already holds the positions:

- A `Shape` in the spatial core carries its own box, computed at create
  and recomputed on update. A node with a shape takes its box from the
  shape (`setShape` sets it, `setBounds` on a shaped node throws, as
  Three derives the bounding box from the position attribute). Every
  geometry gets a shape, not only triangle lists: `createShape` takes
  indices optionally, and a shape without them is box-only in the
  narrowphase. scene.ts no longer computes geometry bounds for the core
  at all; `setBounds` stays for the box-only nodes (instanced meshes
  with explicit population bounds, sprites).
- `updateShape(shape, vertices, stride, posOffset, uvOffset, first)`
  rewrites a vertex range, recomputes the box, drops the triangle BVH
  (rebuilt by the next query) and marks the shape dirty; the flush
  carries the box onto every node holding a dirty shape and refits their
  leaves. `updateVertices` on stream 0 calls it for the same range,
  eagerly, so ranged calls copy only their range.
- Rig: alloy/src/tests/spatial.rs (a shaped node needs no bounds, an
  update moves box and triangles, a ranged update rewrites only its
  vertices, a box-only shape picks by its box) and claim 6 of
  packages/3d/checks/raycast-check.tsx (a mesh deformed out of its rest
  box picks where it moved on the next frame).

Deliberately not done: a bottom-up BVH refit for ranged updates. A
per-frame deformer that also picks per frame pays one rebuild per frame
above the BVH threshold, the same as Three testing every triangle;
worth revisiting only with a measurement. Index updates: there is no
`updateIndices`, and a topology change remains a re-attach. Reading
bounds back from the core into JS to spare the JS scan: an FFI read for
no measurable win, so `geometryBounds` stays the JS-side cache for the
transparent sort and model bounds.
