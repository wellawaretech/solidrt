---
title: Camera and controls extensions
description: The orbit control's gap against the reference set, shaped under okf/design/camera-controls.md - a push through a model (the fly demo's ask, which a bounded dolly can never do), zoom-to-cursor and a dynamic pivot built into the component, an orbit point off the view axis, a follow source with Cinemachine's framing, an occlusion constraint, a shake lane, the map preset and double-tap to focus; with the survey of Three, camera-controls, Babylon, Cinemachine and Godot verified against current sources.
created: 2026-09-17
---

# Camera and controls extensions

The pipeline, the vocabulary and the decisions are in
[camera-controls](../design/camera-controls.md); this item is the 3d
orbit half of the work it lists. The 2d half is
[2d-camera-framing](2d-camera-framing.md); the first-person reference
frame is [3d-first-person-reference-frame](3d-first-person-reference-frame.md).

## Symptom

In the fly connectome demo (a long brain plus nerve cord model) a
two-finger pinch always heads for one point and stops there. The user
wants the pinch to **move through the model** in a direction, "a move,
not a zoom", to reach any region of it.

Why the orbit control stalls, in `packages/3d/src/orbit.ts`: the zoom is
multiplicative (octaves, `zoomAbout` scales the distance by a ratio), so
the eye approaches the anchor geometrically and never reaches or passes
it; and the distance clamps at `minDistance`, where `anchored()` derives
the target from the ratio that survived clamping, so the target stops
too ("once distance pins at a clamp the target stops moving too").

The demo's stopgap today (`demoes/fly/src/index.tsx`, marked TEMPORARY
and pointing here) bypasses the control's axes: a `push` gesture on the
input map moves the target along the screen ray under the fingers by a
fixed world step per octave through `orbit.set`, and a `turn` gesture
rotates eye and target about the world origin by hand. That second
gesture is a requirement of its own: after a push the model's centre is
no longer on the view axis, and `setPivot` projects onto the view axis,
so the control cannot turn about it.

## What SolidRT has

- **`OrbitCamera`** (`orbit.ts`, `components/orbit-camera.tsx`): a
  turntable around a target. Axes `rotate`, `zoom` (a dolly), `pan`;
  `orbitBindings` (drag rotates, pinch and wheel dolly, two fingers,
  Ctrl-drag and right-drag pan, sticks, triggers, arrows); speeds,
  `damping`, distance and elevation clamps, `clampPose`, auto-orbit,
  verbs `set`, `glideTo`, `fit`, `zoomBy`, `panBy`, `rotateBy`,
  `setPivot`. Two hooks the app fills: `zoomAnchor(focal, view)` and
  `rotateAnchor(view)`.
- The component reaches `ctx.scene` (pick, unproject, screenRay, size),
  so in a `<Scene>` it can build the anchor mapping itself. A
  `ViewHandle` has `pick` and `size` but no `unproject`/`project`/
  `screenRay`, so inside a `<View3d>` only a pick-based anchor is
  possible today.
- **`FirstPersonCamera`** (`first-person.ts`): look, walk, rise (fly
  mode), boost; pointer, pad, keyboard. Covers Three's FlyControls
  (minus roll) and PointerLockControls.
- Nothing for a follow source, occlusion, lanes, a push, a map preset,
  trackball, or object manipulation.

## What others offer (verified 2026-09-22 against current sources)

**Three.js core addons.** `OrbitControls`: `zoomToCursor` (default
false) applies to the wheel and the pinch, not the middle-drag dolly or
the keys; multiplicative (`0.95^x`); at `minDistance` the camera stops,
the same stall as ours; `screenSpacePanning` (default true, false pans in
the plane orthogonal to `up`), `enableDamping`, `autoRotate`,
azimuth/polar limits, `minTargetRadius`/`maxTargetRadius` around a
`cursor` point (a target sphere our `clampPose` expresses), configurable
`mouseButtons`/`touches` (one finger rotates, two `DOLLY_PAN`).
`MapControls`: the same class with left pan, right rotate, one finger
pan, two fingers dolly and rotate, screen-space panning off.
`TrackballControls`: no fixed up, two-finger roll. `ArcballControls`:
`cursorZoom` (default false) and `enableFocus` (a double tap raycasts
the scene and glides the pivot to the hit), gizmos, near/far
adjustment. `FlyControls` (roll, drag-to-look), `FirstPersonControls`
(now with damping), `PointerLockControls`; `TransformControls` and
`DragControls` manipulate objects, not the camera.

**camera-controls** (yomotsu). `infinityDolly`: "if the Dolly distance
is less (or over) than the minDistance (or maxDistance), infinityDolly
will keep the distance and pushes the target position instead"; the
overflow moves the target along camera forward, so the eye's speed is
continuous across the floor; wheel, pinch and drag-dolly alike; combines
with `dollyToCursor`. Also `truck`, `forward`, `elevate`,
`setOrbitPoint` (with a focal offset, so the camera does not move),
`setFocalOffset`, `fitToBox` (padding, cover) and `fitToSphere`,
`setBoundary` with `boundaryFriction` and `boundaryEnclosesCamera`,
`colliderMeshes`, `smoothTime`, save/reset, JSON state, rest/sleep
events.

**Babylon.js.** `ArcRotateCamera`: `zoomToMouseLocation` (default false,
wheel only; the pinch has `useNaturalPinchZoom` instead), clamps at
`lowerRadiusLimit` and the target stops (the same stall),
`wheelDeltaPercentage`/`pinchDeltaPercentage` for multiplicative steps,
`targetScreenOffset`, `panningDistanceLimit`, `mapPanning`, `targetHost`
(follow a node; disables panning), `checkCollisions`/`collisionRadius`,
double tap restores the stored state, `zoomOn`/`focusOn` as fit.
Behaviors on it: AutoRotation (idle spin-up), Bouncing (at the radius
limits), Framing (fit on target set, elevation return). Cameras:
Universal, ArcRotate, Follow, Fly, DeviceOrientation, VirtualJoysticks,
the Anaglyph and VR variants, WebXR, and GeospatialCamera.

**Unity.** Cinemachine 3: Orbital Follow (sphere or three-ring orbit, a
radial axis as the zoom scalar, recentering), Position Composer (dead
zone, soft zone, hard limits, lookahead, per-axis damping), Third Person
Follow, Deoccluder (damped return), Confiner, Impulse; no zoom-to-cursor
and no picked-point pivot; no free-fly component. The base runtime ships
no camera controller.

**Godot.** The editor has orbit, pan, zoom and freelook; the runtime has
only `Camera3D`, `SpringArm3D` (instant occlusion pull-in) and `Camera2D`
(drag margins, per-axis smoothing, smoothed limits, offset). Addons
cover the rest (Phantom Camera reproduces Cinemachine; fly and orbit
camera addons).

Takeaway: zoom-to-cursor is one flag in Three and Babylon, but both stall
at the floor exactly as we do; the push exists only in camera-controls,
and only as the overflow past the clamp. Nobody ships always-push.

## Done looks like

Each numbered item is one stage of the design's pipeline on the orbit
control; the shared framing and lane math is written once in core (the
2d item and this one share it, whichever lands first).

1. **Push.** `push?: boolean` on the control and component: a dolly step
   that would cross `minDistance` moves eye and target together by the
   overflow, along the view axis, or along the anchor ray when the step
   is anchored; `maxDistance` the same outward. Speed is continuous
   across the floor. The fly demo's `push` gesture goes; `orbitBindings`
   is unchanged (the pinch and the wheel are still `zoom`).
2. **Built-in anchor and pivot.** `anchor?: "pick" | "plane" | false` on
   `<OrbitCamera>`: the component builds `zoomAnchor` from `scene.pick`
   at the focal point (fallback: the target-depth plane through
   `scene.unproject`) and `rotateAnchor` from a view-centre pick, both
   through `ctx.scene` and `ctx.viewport.size()`. The prop hooks stay as
   overrides. Inside a `<View3d>` only `"pick"` works until `ViewHandle`
   gains `unproject`/`screenRay` (note it in the docs; the view gap is
   its own small item when someone needs it). `AGENTS.md` corrected
   ("only the app can build that mapping" is no longer true).
3. **Orbit point off the view axis.** The offset lane
   (`offset` in viewport fractions, the design's lane) plus
   `setOrbitPoint(point)`: target to the point, offset set so the picture
   does not change. The fly demo's `turn` gesture goes: a drag after a
   push orbits the model's centre through the ordinary `rotate` axis.
4. **Follow source with framing.** `follow(point)` / `unfollow()` on the
   control (the 2d verbs); `follow?: { damping, deadZone, softZone,
   hardLimits, lookahead }` as the option group, the shared framing block
   projecting the point through the pose's basis and fov, the correction
   applied by `slide` (right/up) and a forward move (depth); the orbit's
   own input keeps working around the followed point (Cinemachine's
   Orbital Follow). A drag yields the follow until the next `follow`
   call, as in 2d.
5. **Occlusion constraint.** `occluder?: (target, eye) => number | null`
   on the control (the free distance along the view axis, or null), an
   `occlusion?: { radius }` option on the component that fills it from
   `scene.raycast`; pull-in instant, return damped at the control's ease.
6. **Shake lane.** `shake(strength, duration, frequency?)` on the
   control, in viewport fractions, decaying, summing; applied at push.
7. **Map preset and pan plane.** `mapBindings` in `input.ts` (one finger
   pans, two fingers rotate and dolly, right drag rotates, wheel
   dollies) and `panPlane?: "screen" | "ground"` on the control (ground:
   pan in the plane orthogonal to up, Three's `screenSpacePanning:
   false`).
8. **Double-tap to focus.** A double tap picks the scene and glides the
   pivot there at the current distance (ArcballControls' focus). Wants a
   tap recognizer in core (`ideas.md`); the component derives one until
   then.
9. **Defaults, docs, checks.** Decide the touch-first model-viewer
   defaults (anchor on, push off, follow off) and show them in
   `orbitBindings`' docs and the pick example; `orbit-check.ts` covers
   the push, the lanes, the follow framing and the occlusion return;
   the fly demo moves to the built-ins and its TEMPORARY block goes.

Not in this item: a trackball/arcball control and object manipulation
controls (`ideas.md` lines); the first-person reference frame (its own
item); shots and blends (`ideas.md`, shaped after the pipeline lands).
