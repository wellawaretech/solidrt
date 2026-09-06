---
title: 3d instances as spatial arena nodes - the full-matrix record projection
description: An instanced mesh's records are JS-written floats, so native transitions, clip players and per-instance picking never reach an instance, while every 2d sprite already is an arena node; the core's InstanceProjection has only Pose2D and the full-matrix sibling it anticipates is the missing piece.
created: 2026-09-06
---

# 3d instances as spatial arena nodes

## Symptom

`createInstancedMesh(geometry, material, records)` draws N copies from an
interleaved record buffer the app writes (`setInstances`). The mesh is
one arena node (uModel places the population), the instances are opaque
floats: no instance has a transform the core knows about, so

- a moving instance costs a JS record rewrite per frame (the fleet, the
  crowd, the particle field - the "thousands of dynamic objects" tier
  the spatial core was framed for);
- native transitions (`setTransition`), clip players and root motion
  cannot target an instance;
- picking returns the mesh, by one box around the whole population
  (`bounds`), never the instance struck; `Hit` has no instance index.

The 2d package settled the opposite design in
[2d-spatial-citizenship](../done/2d-spatial-citizenship.md): every live
sprite is an arena node whose `InstanceRecord` sink writes its pose slot
at the core flush, one coalesced buffer write per frame however many
moved, hidden slots zeroed, growth by `retargetRecords`. That sink is
generic; only its projection is 2d. `InstanceProjection` in
`alloy/src/spatial/mod.rs` has the one variant `Pose2D`, and its own
comment calls a full-matrix projection "the anticipated 3d sibling". So
the 3d package is behind its 2d sibling on the core it was the first
consumer of.

## Shape

Two forms on one mesh, exactly 2d's split of node layer and records
layer:

- Core: `InstanceProjection::Matrix` writes the node's world matrix (16
  floats, column-major) into its record slot; per-buffer staging, dirty
  ranges, zeroing and `retargetRecords` are the Pose2D machinery
  unchanged. A `Matrix` slot may sit inside a wider record, so the
  projection carries its float offset within the stride, and the other
  attributes (color, a frame index) stay JS-owned in the same record -
  or in a second instance buffer, the two-slot split 2d uses.
- Package: `addInstance(mesh, transform?)` returns an instance node - a
  `SceneNode` child of the mesh with `setTransform`, `setTransition`,
  `setVisible` (hidden = zeroed slot, zero scale draws nothing) and
  `remove` (its slot recycles). `Hit` gains `instance` for a node-backed
  population; each instance node carries the geometry's bounds (and the
  shared shape) so picking, overlap and sweep resolve per instance.
  `setInstances(records)` stays as the raw escape hatch for motion only
  JS can compute, with the 2d rule: a buffer is node-backed or
  JS-written, never both (the core's staging republishes gap slots).
- Frustum culling stays per mesh (one draw entry, the population box);
  per-instance culling is a zeroed slot, which is the visibility rule
  already.

Comparison: Three's `InstancedMesh.setMatrixAt` is the records form and
its `instanceId` on a raycast hit is the per-instance pick; Unity's GPU
instancing draws GameObjects, so every instance is a node with its own
transform and animation; Godot's `MultiMesh` is records-only with
`MultiMeshInstance3D` as the one node. The node-backed form is Unity's
model over Three's buffer, which is what the 2d package already is.

## Done looks like

A fleet example where a thousand instances spring to new targets through
`setTransition` with zero per-frame JS, a tap reports the instance
struck, and the bench shows one coalesced record write per flush. The
records form's example is unchanged.

## Not in this item

Skinned instances (per-instance palettes), per-instance material params
beyond the record's own attributes, and the [3d-lod](3d-lod.md) swap.
