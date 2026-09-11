---
title: A geometry updated in place keeps picking against its old positions
description: updateVertices re-uploads a stream and drops the cached bounds, but the picking shape the spatial core built at first draw keeps the positions it was built from, so a deforming mesh picks where it used to be until it is re-attached.
created: 2026-09-11
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
that swaps the shape id on every node referencing it. Bounds-only picks
(lines, points) already follow through the bounds drop.

## What it involves

A spatial-core verb (`flux:spatial` and its Rust owner), the entry
bookkeeping in geometry-gpu.ts (the shape is per geometry; the nodes
that set it live in scene.ts), and a rig case: pick after an update
lands on the moved triangle.
