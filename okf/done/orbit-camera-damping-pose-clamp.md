---
title: Orbit camera - zoom damping and a pose clamp hook
description: Two gaps the third-dimension demo hand-rolls around createOrbitCamera - an eased wheel zoom (Three's enableDamping) and a distance-dependent elevation floor (a clampPose hook beside the pan-only clampTarget).
created: 2026-09-07
---

# Orbit camera: zoom damping and a pose clamp hook

`createOrbitCamera` covers drag, pinch, wheel, two-finger pan, auto-orbit
and the clamps. Two things an interactive viewer wants on top are missing,
and `packages/3d/demos/src/the-third-dimension.tsx` writes both by hand -
about 50 of its 800 lines, in the frame loop, the leaf handlers and one
debug command. The demo is the evidence, not the target: the same two
would be hand-written by any app with a wheel and a ground plane.

## Symptom

### 1. A wheel notch is a jump, so apps ease it themselves

A wheel notch reaches the control as an unbracketed `zoom` delta
(through the input map) and applies its octaves to the distance at once.
On a mouse that turns a scroll into a staircase: each notch teleports the
camera and the picture never moves between them.

The demo therefore rebinds the wheel from the orbit's `zoom` to an action
of its own, keeps a pending distance per panel, retargets it on each
notch, and glides toward it in its own `onFrame`:

```
const WHEEL_ZOOM = 0.0015 // exponent per wheel-delta unit, matching the library's sensitivity
const ZOOM_EASE = 9       // e-foldings per second toward the pending distance
const ZOOM_EPSILON = 0.0005
```

The zoom rate lives in the frame loop, apart from the control's own
`zoomSpeed`; nothing fails when one of them changes, the two just drift
apart. (The 2d camera already does this inside the control: a bracketed
zoom delta applies at once, an unbracketed one glides.)

Prior art: Three's OrbitControls has `enableDamping` + `dampingFactor`
(damping the spherical delta, so rotate, zoom and pan all coast, with
`update()` in the render loop as the price). Unity's Cinemachine damps
per axis on the component. Godot ships no orbit control, so its camera
rigs damp in the script - which is exactly the position we are in.

### 2. A clamp that depends on the pose has nowhere to live

`minElevation`/`maxElevation` are values (live values, since props are
read where they apply, but values). A clamp that is a FUNCTION of the
pose cannot be expressed: the demo wants the eye to stay above the floor,
and eye height is `target.y + distance * sin(elevation)`, so the
elevation floor tightens as the zoom pulls out.

It ends up in the frame loop, one call per panel per frame:

```
let holdAboveFloor = (cam: OrbitCameraHandle) => {
  let pose = cam.pose()
  let minElevation = Math.asin(clamp((EYE_MIN_Y - KNOT_CENTER[1]) / pose.distance, -1, 1))
  if (pose.elevation < minElevation) cam.set({ elevation: minElevation })
}
```

A reactive `minElevation` getter cannot replace it: the value depends on
`distance`, which is plain state by design (the structure/motion split),
so nothing re-runs when a zoom changes it. And `zoomAbout` clamps only
the distance - it never re-clamps the elevation - so even a correct
value would not be re-applied on the write that invalidates it. The
frame-loop correction is a frame late by construction, which is visible
as a slight sink-and-recover when a zoom-out crosses the floor.

`clampTarget` exists but is pan-only by contract ("Zoom and rotation do
not consult it"), so it is not the hook.

No engine ships this one either: Three, Unity and Godot all offer value
clamps only. The hook is the minimal form, and it is what a viewer with a
ground plane, a table top or a bounded stage needs.

## Done looks like

**Damping.** A `damping` option (seconds, or e-foldings per second - one
number, off by default) that makes zoom - and, decided at design time,
rotation and pan - coast to the target pose inside `update(dt)`, framerate
independent (`1 - exp(-rate * dt)`, not a fixed fraction). The demo then
drops its `dolly` rebinding and deletes its pending distance, its glide,
its constants and the `entry.zoom = null` line in the `camera` debug
command.

The design point that is easy to miss: `active()` is the frame-loop gate
(`<OrbitCamera>` mounts an `onFrame` only while it is true, so a paused
camera keeps the app demand-driven idle). Damping motion must therefore
raise `active()` while it settles and drop it when it lands, which means
one signal write at each end - the pose itself stays plain state. Without
that, a damped zoom on a paused camera never advances.

Also to settle: whether a parked pose (`set({ distance })`, what the debug
commands do) snaps or coasts. Snapping is what a repeatable
park-then-snapshot needs, so `set()` should land immediately and clear any
motion in flight, with damping applying to input only.

**Pose clamp.** A `clampPose` hook called after every pose mutation (drag,
zoom, pinch, pan, auto-orbit and `set()`), taking the whole pose and
returning what to use:

```
clampPose?: (pose: { azimuth, elevation, distance, target }) =>
  { azimuth?, elevation?, distance?, target? } | void
```

The demo's floor becomes three lines on the `<OrbitCamera>` props and
`holdAboveFloor` disappears, correct on the write instead of a frame
later. Since it runs on every mutation, whether `clampTarget` folds into
it (it is the pan-only special case) is the shape question to answer
first; there is no compatibility reason to keep both.

## Involves

`packages/3d/src/orbit.ts` (the pose write path is already funnelled
through `clampPose()`/`zoomAbout`/`slide`, so both land in one place),
`components/orbit-camera.tsx` for the props, the AGENTS.md camera-control
section, and the demo cut-over as the check that the hand-rolled versions
actually go away. `packages/2d`'s camera controller shares the
`active()`/`update(dt)` shape, so whatever damping looks like here should
read the same there.

## Non-goals

- Rotation inertia (a flick that keeps spinning). Different feel,
  different decision; damping a target pose is not a velocity model.
- A tap recognizer. The demo also hand-rolls tap-to-pause, but that is
  core's gap, already captured in ideas.md (`createPress` sits in
  packages/components, so no package-level control can reach it).

## Outcome

Landed 2026-09-11 with the glide item. `damping` is ON by default, not
off as written above: the 2d camera has always glided a wheel notch, and
the same app should read the same in 2d and 3d, so `damping` became one
knob on all three controls (a multiple of the built-in settle time, 0 =
apply at once). It eases unbracketed deltas on every axis - a wheel notch,
a key step - never a gesture-bracketed delta or a rate, and never the
first-person control's look (mouse motion under pointer lock). `clampPose`
replaced `clampTarget` outright (no compatibility reason to keep both),
runs after every write including a glide's frames, and the demo's floor
is one prop; its wheel rebinding, pending distance, constants and debug
command line are gone. `set()` snaps, as decided above.
