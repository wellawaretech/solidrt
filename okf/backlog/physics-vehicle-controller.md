---
title: Vehicle controller on the physics core
description: physics-core stops at rigid bodies, primitive colliders and (stage 2) a character controller; a driving game needs a raycast vehicle - a chassis body with wheel rays, suspension springs, engine and brake forces and tyre friction stepped inside the solver, which is per-wheel-per-substep work no app can do in JS. Rapier has no vehicle module of its own; shape it as the physics core's first higher-level controller after the core lands.
created: 2026-08-30
---

# Vehicle controller on the physics core

## Symptom

[physics-core](physics-core.md) lists vehicles under "Not in this item ...
returns as its own item when a consumer exists". A kart or car game IS the
consumer, and it is the one game genre that cannot be approximated with
the primitives stage 1 delivers: a kart as a dynamic box on a track
tumbles at the first bump, and a kinematic body driven by app code has no
suspension, no weight transfer and no drift.

The standard shape, in every engine since Bullet's `btRaycastVehicle`, is
a raycast vehicle: the chassis is one dynamic rigid body; each wheel is a
ray (or short shape-cast) from a chassis-local hardpoint downward, its hit
distance driving a spring-damper that pushes the chassis; engine force,
brake force and steering angle apply per wheel along the wheel's forward
and side axes with a friction model (slip ratio -> force, a simple
Pacejka or a clamped linear one). All of it runs inside the physics step,
per wheel per substep, which is why it must live in the core: the
interpreter cannot run four wheels at 120 Hz beside the solver, and a
one-step-late JS controller fed by frame-batched poses oscillates.

Rapier ships no vehicle module (the `rapier3d` crate has a
`DynamicRayCastVehicleController` in its `control` module - a port of
Bullet's, the size of a few hundred lines; verify it is in the version the
core embeds and whether it exposes what is listed below). That is the
implementation to wrap, not to write.

## Shape

- JS sends intent, O(changes): `createVehicle(body, { wheels: [{ position,
  radius, suspensionRestLength, stiffness, damping, maxTravel, steering?:
  boolean, drive?: boolean }], ... })`, then per input event `setEngineForce`,
  `setBrake`, `setSteering` (a gamepad axis read is one write per frame at
  most, and only when it changed).
- Core steps the vehicle inside the world step. Chassis pose arrives
  through the body's node binding as any body's does; per-wheel state
  (rotation, suspension length, in-contact, skid) is read back for the
  wheel meshes and effects, batched per frame like collision events - the
  wheel MESHES are ordinary child nodes the sink writes, so wheels spin and
  bounce with zero per-frame JS.
- Surface response per collider (friction, a "drivable" flag, a speed
  multiplier for boost pads) comes from the collider's material, which
  stage 1 defines.

Stages: none of its own until physics-core stage 1 exists; then one stage,
gated on a real game project that drives the API the way the stage-0
game shapes the core (physics-core's own rule).

## Done looks like

A kart on a looping track accelerates, brakes, steers, drifts and lands
from a jump at a fixed physics rate independent of the frame rate,
deterministic under the record/playback replay, with the app writing
inputs only.

## Not in this item

Motorbike lean, tracked vehicles, aerodynamic models, tyre thermals,
anything in the game's own tuning (kart classes, boosts) - app data on
top of the controller's parameters.
