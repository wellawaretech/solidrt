---
title: Collision queries on the spatial index - overlap, shape sweep, move-and-slide
description: The only collision tool a game has is the raycast against an undrawn collider mesh, so a character walking into a wall, picking up an item or standing on a slope needs per-frame JS geometry every app writes badly; give the spatial core overlap and swept-shape queries over the index it already keeps, and the 3d package a move-and-slide over them, so the lightweight collision tier games need arrives without a physics engine.
created: 2026-09-06
---

# Collision queries on the spatial index

## Symptom

`@solidrt/3d` answers one spatial question: what does this ray hit. The
"physics-collider pattern" in `packages/3d/AGENTS.md` (a low-poly mesh on an
undrawn layer, `raycast` with `{ layers }`) covers a ground probe and a
click, and nothing else a game asks every frame:

- a capsule walking into a wall (block, then slide along it),
- a sphere of interest (which pickups, enemies or triggers are within
  reach),
- a moving body sweeping through a corridor (did it pass through anything
  between two frames).

Today each is JS geometry per frame per body: a hand-rolled sphere-vs-box,
or rays fanned out from the character and reconciled by hand. That is the
per-body-per-frame work [3d-differentiators](../notes/3d-differentiators.md)
puts below the interpreter line, and the spatial core already holds every
piece of the answer: world matrices, per-node bounds, a BVH, and per-geometry
triangle copies for the pick narrowphase.

[physics-core](physics-core.md) explicitly parks this tier here ("many games
need only that, and it must not be gated on this item") and
[spatial-core](spatial-core.md) names overlap and sphere-cast queries as
consumers of its index that never got an item. This is that item. The 3d
roadmap ranks it for a game between frustum culling and low-end GPU
performance: without it nothing beyond a fly-through is playable.

## What the three engines do

Three has `Raycaster` only; every Three game reaches for a physics library
or three-mesh-bvh's `shapecast` for the rest. Godot's
`PhysicsDirectSpaceState3D` has `intersect_shape`, `cast_motion` and
`collide_shape`, and `CharacterBody3D.move_and_slide` sits on top. Unity
has `Physics.OverlapSphere/OverlapBox`, `SphereCast/CapsuleCast`, and
`CharacterController.Move`. The common tier is two queries plus one
controller, and the queries are pure geometry against the same acceleration
structure picking uses.

## Shape

Core, in `alloy/src/spatial/` beside `pick.rs`, over the same BVH and the
same per-geometry shapes (a query respects layers and visibility exactly as
`pick` does, so an undrawn collider mesh answers all three):

- `overlap(shape, { layers? })` - the nodes whose bounds (broadphase) and
  triangles (narrowphase) intersect a sphere, box or capsule; returns node
  ids plus the closest point and penetration normal per hit.
- `sweep(shape, motion, { layers? })` - the shape moved along a vector: the
  first time of impact, the hit normal and the node, or "clear". Sphere and
  capsule; the box form follows on demand.
- Both are one FFI call returning a packed hit array, the raycast's
  marshalling shape.

Library, in `@solidrt/3d`:

- `scene.overlap(...)` / `scene.sweep(...)` mirroring `scene.raycast`.
- `moveAndSlide(node, capsule, motion, opts)` - Godot's name: sweep,
  advance to the hit, project the remaining motion onto the hit plane, repeat
  up to a small iteration cap; reports the floor normal so the app knows
  grounded versus airborne. One call per controlled body per frame, so it
  stays O(bodies), not O(scene).

## Done looks like

A capsule-controlled character walks a level built from undrawn collider
meshes: blocked by walls, sliding along them, climbing a ramp, dropping
off a ledge, with its per-frame JS being the input read and one
`moveAndSlide`. Pickups light up through one `overlap` per frame. No
physics engine involved; when [physics-core](physics-core.md) lands its
character controller may replace `moveAndSlide` for apps that have dynamics
anyway.

## Not in this item

Rigid bodies, joints, contact resolution between two moving bodies
(physics-core). Mesh-vs-mesh overlap (convex pairs) beyond triangle-vs-shape.
Navmesh.
