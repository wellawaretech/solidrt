---
title: Spring bones - secondary motion solved in the spatial core
description: A native chain solver for hair, skirts, tails and cables (Unity's SpringBone, Godot's SpringBoneSimulator3D, three-vrm's VRMSpringBoneJoint), declared once per model and stepped after the frame's JS as a pose post-pass; frame-rate exact by construction, with sphere, capsule and plane colliders, so a rigged character costs zero per-frame JS for its secondary motion.
created: 2026-10-19
---

# Spring bones

## Problem

Every VRM avatar and most rigged characters carry chains that lag behind
their animated parent: hair, a hood, a skirt, a tail, a cable. The
spatial core owns the pose (clip players advance before the frame's JS),
the world matrices and the collision math, but has no chain solver, so a
port runs three-vrm's joint update in JS: 34 joints, one `getTransform`
per ancestor of every chain root, 34 `setTransform` writes, about 0.8 ms
per frame on a desktop - nearly all of the character's per-frame JS. The
feedback that raised it also asked for `worldInto`, which the solver made
it compose by hand.

## Done looks like

- `createSpringBones(model, { colliders, springs })` in `@solidrt/3d`:
  per-joint settings in the VRM 1.0 / three-vrm / Godot vocabulary
  (stiffness, drag, gravity power and direction, hit radius, tail), an
  optional center node, colliders shared by index. `reset()`,
  `setJoint()`, `setForce()` and `dispose()`. The declaration lives on
  the model node and re-applies on every scene enter, like
  `setTransition`.
- The core steps every chain once per frame AFTER the frame's JS and its
  flush, in the draw tick ahead of the spatial flush: secondary motion is
  a function of the final primary pose, and the frame's JS is a primary
  producer (a look-at, an IK, a controller's move). Primary movers keep
  stepping before the frame's JS; this is a second class, the pose
  post-pass, not an exception. Unity runs SpringBone in LateUpdate and
  Godot runs SkeletonModifier3D after process for the same reason.
- Same motion on a 50 Hz TV, a 60 Hz desktop and a 120 Hz phone, with no
  added latency and no judder (below).
- Sphere, capsule and plane colliders, `inside` on sphere and capsule.
- A settled chain asks for no frames. A chain whose node dies drops.
- `worldInto` exported.

## Decisions

**Placement after the frame's JS.** See above. Consequence, documented:
`getTransform`/`worldPosition` on a spring joint during the frame read
last frame's solve, and the chain owns its joints (a JS write to one is
overwritten every frame, as a player's joint is at the next advance).

**Frame-rate exactness without substeps.** three-vrm, UniVRM and Godot
step with the raw frame delta: `tail += (tail - prev) * (1 - drag) +
restDir * stiffness * dt + gravityDir * gravityPower * dt`. The drag is
applied per step, so the same values damp twice as much per second at
120 Hz as at 60 Hz. A fixed 60 Hz substep fixes that but needs
interpolation to avoid judder on 50/90/120 Hz displays, and interpolation
costs one substep of latency, the very lag the placement decision avoids.
Instead the core steps with the frame delta through the one continuous
flow that agrees with three-vrm's map on every whole 1/60 s: the map is
affine in (tail, velocity) with eigenvalues 1 and keep = 1 - drag, so it
has a unique real logarithm, and the flow over any dt is its closed form.
Two half steps compose to one step exactly; a 60 Hz step is three-vrm's
bit for bit (the authored values mean what they meant). Only the
nonlinear projections (bone length, colliders) are per step, as in every
engine; a long frame is split into steps of at most the reference step
so colliders do not tunnel. Derivation and constants in
`alloy/src/spatial/springs.rs`.

**Scale.** Bone length and hit radius follow the bone's matrices every
step (an ancestor scale scales them), collider radii follow their node's
scale; three-vrm 2.x does the same.

**VRM reader stays out.** The app parses `secondaryAnimation` or
`VRMC_springBone` and hands the joints over; a `@solidrt/3d/vrm` reader is
a separate, additive item.

**Authored settings and the runtime force are two things.** `setJoint`
edits a joint's authored settings (the asset's: stiffness, drag, gravity
power and direction, hit radius; VRM's gravity is a per-joint shaping
tool, not physics, and often not down). Wind is one runtime vector per
chain, `setForce`, added to every joint's gravity in the kick, as UniVRM
1.0's ExternalForce (per model) and Godot's external_force (per
simulator) are, and Dynamic Bone's Force beside its Gravity; three-vrm has
only the authored gravity and its wind demos mutate it per joint, the
path this avoids. The tail and the rest rotation are structure and stay
as created.

## Work

1. Core: `alloy/src/spatial/springs.rs` (types, flow, step, colliders),
   `Spatial::create_spring/destroy_spring/reset_spring/advance_springs`,
   tests in `alloy/src/tests/spatial_springs.rs`.
2. Plugin: `flux:spatial` `createSpring/destroySpring/resetSpring`, the
   step in the draw tick; `packages/flux-types/gui/spatial.d.ts`.
3. Package: `packages/3d/src/springs.ts`, the enter hook on the model
   node, `worldInto` export, AGENTS.md section after Root motion.
4. Probe `probes/spring-bones-probe.tsx`: a chain under a swinging bone,
   colliders, settle demand read through the control API.

## Findings

- The core at whole 60 Hz steps reproduces the port's JS solver (three-vrm's
  VRM 0.x update, an independent implementation) to the fifth decimal over
  eight steps in four configurations (collider and gravity on and off):
  the flow, the length and collider projections, the rotation derivation
  and the center handling all agree. The one differing digit is f32
  against f64 state.
- The cross-rate deviation of a quarter-turn step response on a 20 cm
  two-bone chain (60 Hz against 120 and 50 Hz, sampled every 100 ms):
  4 mm with one projection per frame, 0.85 mm at 2 substeps, 0.33 mm at
  4, 0.11 mm at 8. Four substeps per reference step (240 Hz) was chosen:
  invisible, and the solve costs microseconds.
- A frame delta a rounding hair over a whole number of substeps must not
  take one substep more: `ceil(dt / substep)` did, and a 60 Hz frame
  stepped five substeps on frames where the clock stamp's float noise
  pushed it over four, which doubled the projection rate between frames.
  A thousandth of a substep of slack fixes it.
- Priming at the first advance is wrong: a chain created and whose
  parent is then turned before any frame ran primes at rest under the
  turned pose and never swings (the app test caught it; the probe hid
  it because its anchor swings every frame). The tails are placed at
  creation and at reset, from the world matrices as they stand (pending
  writes included), as three-vrm's setInitState does.
