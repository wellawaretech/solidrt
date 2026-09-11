---
title: A 3d camera cannot be told to go somewhere, or to frame something
description: createOrbitCamera and createFirstPersonCamera move only by input deltas and snapping set(), so "show me this object" and "return to the default view" are app-side loops; the 2d camera has had glideTo and fit(rect) since it shipped, and every 3d tool has frame-selection.
created: 2026-09-11
---

# A 3d camera cannot be told to go somewhere, or to frame something

## Symptom

Both 3d controls move two ways: an input delta (`rotateBy`, `zoomBy`,
`panBy`, `lookBy`, `moveBy`) and `set(pose)`, which snaps. There is
nothing in between, so two ordinary asks have no answer:

- **Go to a pose.** "Reset view", "cycle to the next camera angle",
  "focus this part" all jump. The app writes its own easing loop against
  `set()` and has to gate `active()` itself, or it accepts the cut.
- **Frame a thing.** Given bounds, place the camera so they fill the
  view. The package already produces bounds everywhere -
  `geometryBounds`, `model.bounds`, the cull box, `box3Helper(bounds)` -
  and offers no way to point a camera at one. The math (fit the sphere
  to the vertical and horizontal fov, respect `minDistance`/`maxDistance`,
  keep the current azimuth and elevation) is the kind an app gets subtly
  wrong, especially the aspect-ratio half.

`@solidrt/2d`'s control has had both since it shipped: `glideTo(x, y,
zoom?)` and `fit(rect?, { glide })`, plus `viewRect()` to read back what
is framed (`packages/2d/src/camera2d.ts`). So the same app in 2d and 3d
gets two different answers to the same question.

## Three, Unity, Godot

- **Three**: `OrbitControls` has neither, which is why
  `yomotsu/camera-controls` is the de facto replacement in the ecosystem:
  its headline features are `fitToBox`/`fitToSphere` and transitions on
  every setter (`setLookAt(..., enableTransition)`, `dollyTo`,
  `moveTo`). A gap the ecosystem routed around is a gap.
- **Unity**: Cinemachine frames targets as a first-class job (Framing
  Transposer, Target Group with per-target weight and radius), and the
  editor binds Frame Selected to F.
- **Godot**: ships no camera rig, so scripts tween the camera; the editor
  binds Frame Selected to F.

Frame-selection is universal in the tools and absent in two of the three
runtime APIs; our 2d already picked the side worth being on.

## Done looks like

On both controls, sharing one motion mechanism:

- `glideTo(pose, opts?)` easing to a pose inside `update(dt)`,
  framerate independent, cancelled by any input and by `set()`.
- `fit(bounds, opts?)` on the orbit control: keep azimuth and elevation,
  move `target` to the bounds centre and `distance` to what frames the
  bounding sphere at the current fov and aspect, then clamp as usual.
  `{ glide: true }` routes it through `glideTo`, matching 2d's `fit`.
  The first-person control's equivalent is "stand back and look at it",
  which is a different verb and can wait for someone to ask.

**Design this with the damping item, not after it.**
[orbit-camera-damping-pose-clamp](orbit-camera-damping-pose-clamp.md)
needs exactly the same machinery: a motion advanced in `update(dt)` that
must raise `active()` while it runs and drop it when it lands, or a
damped camera on a demand-driven app never advances. Two separate
mechanisms for "the pose is moving on its own" would be a mistake. The
two are different in intent (damping smooths input, a glide is a
commanded move) and identical in plumbing, and the 2d control already
runs both through one `update(dt)` with one `active()`.

Settle at the same time: whether `set()` cancels a glide (2d says yes, a
write cancels a glide or fling in flight), and whether `fit` without
`glide` snaps (it should, so a park-then-snapshot stays repeatable, which
is the same rule the damping item lands on for `set()`).

## Non-goals

- A chase or follow rig. 2d has `follow`/`unfollow` with `followSpeed`
  and a dead zone; the 3d counterpart is a third-person rig, deliberately
  deferred in [3d-roadmap](../notes/3d-roadmap.md) until a third-person
  game asks. Not smuggled in here.
- Inertia and flings. Already a stated non-goal on the damping item:
  damping a target pose is not a velocity model.

## Involves

`packages/3d/src/orbit.ts` and `first-person.ts` (both funnel pose writes
through a clamp and push, so one motion slot fits beside it),
`components/orbit-camera.tsx` for the `active()` gate that mounts the
frame loop, and the AGENTS.md camera-control section.
