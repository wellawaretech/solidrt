---
title: Physics core - an embedded engine as a producer into the spatial arena
description: Rigid-body physics is per-body-per-frame and per-contact work, below the interpreter line, and nothing provides it, so games needing dynamics are blocked or hand-roll it badly. Embed a proven Rust engine (Rapier) behind a thin core module that steps on a fixed timestep and writes body poses into spatial nodes, with JS sending intent (bodies, impulses, joints) and collision events returning frame-batched; learn the API shape in a real game at rung 2/3 first.
created: 2026-08-24
---

# Physics core

## Symptom

There is no physics anywhere in the stack, and the interpreter rules out
writing one in JS ([3d-differentiators](../notes/3d-differentiators.md)
names physics explicitly in the per-frame group). An app wanting dynamics
today either fakes it (constant-velocity + the spatial overlap queries
that do not exist yet either) or ships its own engine down the escape
ladder per app - exactly the "every app would otherwise write it" test
that routes work into core.

## Shape: embed, do not write

A physics solver is a decade of someone else's hard-won correctness;
building one is not our comparative advantage. Rapier is pure Rust (no FFI
of its own, ordinary cargo dependency), actively maintained, and has an
enhanced-determinism mode - which pairs with the record/playback and
frame-clock machinery into deterministic replays, a genuine differentiator
the browser libraries cannot offer.

The module is a thin producer into the spatial arena, the same relationship
[animation-core](animation-core.md) has:

- **JS sends intent**, O(changes): create/destroy rigid bodies and
  colliders, set velocities, apply impulses/forces, create joints, bind a
  body to a spatial node. A binding means: after each step, the body's
  pose (position + quaternion; scale is not physics state) is written to
  the node's local TRS through the existing `set_transform` path, before
  `flush()`. Spatial needs zero changes; sinks, picking and sorting just
  see moved nodes.
- **Core steps on a fixed timestep** accumulated from the frame clock
  (fixed dt is what makes determinism and stability real; interpolation of
  the render pose between steps is a decide-early option, not a bolt-on).
- **Events return frame-batched**: collision enter/exit and contact data
  buffered per step and delivered once per frame, the pointer-move
  pattern, through forge::events.

Kinematic bodies close the loop in the other direction: a JS- or
animation-driven node whose binding writes the node pose INTO the body
each step, so moving platforms and animated characters push dynamic
bodies.

## The two decisions to make first

- **Layering.** The solver itself is engine-free, which says `forge`; the
  pose sync needs the spatial arena, which lives in `alloy`. Either forge
  owns the stepping core and alloy adapts it (the "flux foundation,
  lattice adapts" instinct, and it keeps physics usable headless - server
  authority, tests), or it sits beside `spatial/` in alloy and headless
  use is deferred. Settle this before code; it decides the crate graph.
- **Binary size.** Most solidrt apps are not games. Measure what Rapier
  adds to a dist build; if it is not noise, it becomes a cargo feature
  like SPEECH, with the default set by the measurement.

## Staging

Stage 0 - shape discovery at rung 2/3. Run Rapier via `flux:wasm` (or
`flux:ffi` on desktop) inside a real game project and let the game's needs
write the API surface. This is how instancing got its shape right (the
racing demo's shim); the sync-into-scene-graph code written here IS the
draft of the core binding contract, and the exercise doubles as demand
evidence. Cheap, disposable, no platform commitment.

Stage 1 - the core module. World create/step, rigid bodies (dynamic,
kinematic, fixed), primitive colliders (box, sphere, capsule), node
bindings, impulses/velocities, collision events, gravity. Done looks like:
the stage-0 game runs on it with its wasm/ffi shim deleted, poses arrive
with zero per-frame JS, and a determinism check replays a recorded run
bit-identically.

Stage 2 - on demand: joints, trimesh/convex colliders (the spatial pick
shapes already hold per-geometry position copies - reuse is an open
question, not a promise), scene queries beyond spatial's own
(shape-casts), character controller.

## Not in this item

Writing a solver, soft bodies, cloth, fluids, vehicles, navmesh/AI (a
different item entirely), 2D physics (Rapier has a 2D twin; wait for a
consumer). The lightweight tier - overlap and sphere-cast queries on the
spatial index, "collision without a physics engine" - is already named in
[spatial-core](spatial-core.md) as a spatial query item and stays there;
many games need only that, and it must not be gated on this item.
