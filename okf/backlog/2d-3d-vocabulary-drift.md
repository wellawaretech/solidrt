---
title: Two names drifted apart between @solidrt/2d and @solidrt/3d after the symmetry passes
description: Reading a camera control is camera() in 2d against pose() in 3d with a Camera2dPose type nothing returns, and "is it playing" is a boolean field against a method returning names; both landed separately after the 08-31 and 09-06 unison reviews.
created: 2026-09-11
---

# Names that drifted apart between the 2d and 3d packages

The two unison reviews (08-31 and 09-06) closed with the vocabulary
matched. Everything below shipped after them, each in its own item, so
nothing here was a decision anybody made twice. Grouped because it is one
pass over one surface, the same shape as
[3d-naming-inconsistencies](3d-naming-inconsistencies.md). No
compatibility constraint on either of them.

Excluded on purpose: the asymmetries the 09-06 review recorded as
deliberate (creation grammar, remove-versus-destroy, one-bag `setSprite`
against per-aspect 3d setters, the `Sprite`/`Group`/`worldPosition` name
collisions between the packages). Those are settled; do not reopen them.

Two items from this pass have landed since it was filed: 2d's
`SpriteHandlers` became `LayerHandlers`, restoring the pair with 3d's
`SceneHandlers` (and matching `LayerPointerListener` against
`ScenePointerListener`), and 2d's `RayHit` became `Hit`, the name 3d
already used for the same contact.

## 1. camera() against pose(), and a pose type nothing returns

Reading the current state of a camera control:

- 2d: `Camera2d.camera(): CameraUpdate`.
- 3d: `OrbitCamera.pose()` and `FirstPersonCamera.pose()`, plus `eye()`
  (and `forward()` on the first-person control).

Part of this is inherent and should stay: in 3d the control's parameters
(azimuth, elevation, distance, target) are not the camera's state, which
is why `pose()` and `eye()` are two calls. In 2d they are the same four
numbers, so one call can serve both.

What is drift is the missing half of the read/write pair. 2d declares
`Camera2dPose` as what `set()` takes, and nothing ever returns one; 3d
declares `OrbitPose` for `set()` and `pose()` returns the filled form. So
in 2d you write one type and read another, and the verb differs from 3d
for no reason a reader can see.

Smallest fix that restores the pair: `Camera2d.pose(): Required<Camera2dPose>`
beside `camera()`, the two differing exactly as 3d's `pose()` and `eye()`
do (control parameters against what the camera target is told). Whether
`camera()` then survives is the call to make - it is what the control
pushes to its targets, which is a real second role.

Three, Unity and Godot give no steer: none of them ships a 2d camera
control at all (Godot's `Camera2D` is a node with a position, not a
controller), so the pairing argument is internal.

## 2. playing, the field, against playing(), the method

- 2d `SpriteAnimation`: `readonly playing: boolean`, plus
  `readonly frame: number`.
- 3d `Mixer`: `playing(): string[]`, plus `clips: string[]` as a field.

Same word, two call conventions and two types, and each package is
internally mixed as well (2d reads state through fields, 3d through a
method except for `clips`). A 2d clip has one thing playing and a 3d
mixer blends several, so `string[]` against `boolean` is honest; the
convention should still be one convention.

Three's `AnimationAction` exposes `isRunning()`, Unity's
`Animator.GetCurrentAnimatorStateInfo`, Godot's
`AnimationPlayer.is_playing()` - the engines all use a method, which
argues for `playing()` in both and `frame()` with it.

## Not a finding: pick's return type

2d's `pick(x, y)` returns bare `Sprite[]` where 3d's returns `Hit[]`, and
that is correct. 3d's pick is a camera ray, so a distance orders the
results and `face`/`uv` mean something; 2d's is a point-in-rect test,
where the point is the query itself and a normal would be invented.
Unity's `OverlapPoint` and Godot's `intersect_point` return the objects
alone for the same reason. Recorded here so it does not get re-flagged as
drift by the next pass.

## Shape gate

Each rename carries the standing gate from `packages/2d/CLAUDE.md` and
`packages/3d/CLAUDE.md`: the Three/Unity/Godot comparison in the
proposal. Item 1 has no engine precedent (nothing to copy); item 2 does,
and it is cited above.

## Involves

`packages/2d/src/camera2d.ts` and `animation.ts`,
`packages/3d/src/mixer.ts`, both index re-export lists, and both
AGENTS.md files where the names appear in the camera and animation
sections.
