---
title: Four names drifted apart between @solidrt/2d and @solidrt/3d after the symmetry passes
description: The same element-handler type is SpriteHandlers in one package and SceneHandlers in the other, a ray contact is RayHit against Hit, reading a camera control is camera() against pose(), and "is it playing" is a boolean field against a method returning names; each landed separately after the 08-31 and 09-06 unison reviews.
created: 2026-09-11
completed: 2026-09-11
---

# Four names drifted apart between the 2d and 3d packages

The two unison reviews (08-31 and 09-06) closed with the vocabulary
matched. Everything below shipped after them, each in its own item, so
nothing here was a decision anybody made twice. Grouped because it is one
pass over one surface, the same shape as
[3d-naming-inconsistencies](3d-naming-inconsistencies.md). No
compatibility constraint on any of them.

Excluded on purpose: the asymmetries the 09-06 review recorded as
deliberate (creation grammar, remove-versus-destroy, one-bag `setSprite`
against per-aspect 3d setters, the `Sprite`/`Group`/`worldPosition` name
collisions between the packages). Those are settled; do not reopen them.

## 1. SpriteHandlers is not a sprite's

`packages/2d/src/layer.ts` exports `SpriteHandlers`: the five element
handlers (`ElementPointerEvent`) you spread onto whatever leaf shows a
view's texture. `packages/3d/src/scene.ts` exports the identical type,
member for member, as `SceneHandlers`.

The 2d name is the wrong noun: the value is the VIEW's handlers, held at
`ViewHandle.handlers`, and a sprite never has one. The package already
gets the neighbouring name right - `LayerPointerListener` pairs with 3d's
`ScenePointerListener` - so the fix is `LayerHandlers`, which restores
the pair:

| role | 2d | 3d |
| --- | --- | --- |
| listener at the root of the walk | `LayerPointerListener` | `ScenePointerListener` |
| element handlers for the leaf | `SpriteHandlers` | `SceneHandlers` |

Three, Unity and Godot have nothing to say here: none of them hands an
app a handler bag to spread onto a host element, because none of them
composites into a host UI tree. This one is ours to be consistent about.

## 2. RayHit against Hit

A 2d `raycast` returns `RayHit[]`, a 3d `raycast` returns `Hit[]`, and
the shapes are the same idea: the struck object, the distance along the
ray, the point and the normal. 3d also returns `Hit[]` from `pick`, which
is why its name is the general one.

Prior art splits, so the argument is internal rather than borrowed:
Unity has `RaycastHit` and `RaycastHit2D`, Godot returns an untyped
dictionary from `intersect_ray`, and Three returns `Intersection[]` (the
shape our 3d `Hit` copies, `face` and `uv` included). A porter coming
from Unity would expect the pair to differ by dimension suffix, not by
prefix.

Note what is NOT a finding here: 2d's `pick(x, y)` returns bare
`Sprite[]` where 3d's returns `Hit[]`, and that is correct. 3d's pick is
a camera ray, so a distance orders the results and `face`/`uv` mean
something; 2d's is a point-in-rect test, where the point is the query
itself and a normal would be invented. Unity's `OverlapPoint` and
Godot's `intersect_point` return the objects alone for the same reason.
Leave it.

## 3. camera() against pose(), and a pose type nothing returns

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

## 4. playing, the field, against playing(), the method

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

## Shape gate

Each rename carries the standing gate from `packages/2d/CLAUDE.md` and
`packages/3d/CLAUDE.md`: the Three/Unity/Godot comparison in the
proposal. Items 1 and 3 have no engine precedent (nothing to copy);
items 2 and 4 do, and it is cited above.

## Involves

`packages/2d/src/layer.ts` (the type and its uses in `views.ts`,
`dispatch.ts`, the components), `camera2d.ts`, `animation.ts`,
`packages/3d/src/mixer.ts`, both index re-export lists, and both
AGENTS.md files where the names appear in the pointer and camera
sections.

## Outcome (2026-09-11)

All four landed as renames in @solidrt/2d; the 3d package did not move.

- `SpriteHandlers` is `LayerHandlers`; `RayHit` is `Hit`.
- `Camera2d.pose(): Required<Camera2dPose>` sits beside `camera()`, which
  stays: it is what the control pushes to its targets (pivot included)
  and what projectCamera takes, the 3d `eye()` role.
- `SpriteAnimation.frame()` and `playing()` are methods. The mixer's
  `clips` stays a field on purpose: it describes the model, it is not
  clock state, the same line the sprite clip draws between its options
  and its clock.
