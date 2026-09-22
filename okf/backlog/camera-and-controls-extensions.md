---
title: Camera and controls extensions
description: SolidRT has two stock controls (OrbitCamera, FirstPersonCamera) where Three.js and Babylon.js ship many with more options; the concrete gap is moving through a model (dolly / fly-through, which a zoom can never do), then zoom-to-cursor and a dynamic pivot as built-ins - the survey below is from memory and needs research before it is shaped further.
created: 2026-09-17
---

# Camera and controls extensions

> **NEEDS MORE RESEARCH.** Everything under "What others offer" is from
> memory, not checked against current docs. Before this is planned, verify
> it against Three.js (core addons and `camera-controls`), Babylon.js
> (cameras and camera behaviors), Unity (Cinemachine) and Godot (addons),
> and turn it into a feature-by-feature comparison with `OrbitCamera` and
> `FirstPersonCamera`. The list is known to be incomplete.

The first-person asks (a boost action, a reference frame) are shaped
without research in
[3d-first-person-boost-and-frame](3d-first-person-boost-and-frame.md).

## Symptom

In the fly connectome demo (a long brain + nerve cord model) a two-finger
pinch always heads for one point and stops there. The user wants the pinch
to **move through the model** in a direction - "a move, not a zoom" - to
reach any region of it.

Tried in the demo, and it does not do this: zoom-to-cursor plus a dynamic
pivot, written by hand with `zoomAnchor`/`rotateAnchor`. It still stalls,
for two reasons in `packages/3d/src/orbit.ts`:

1. The zoom is multiplicative (octaves: each step scales the distance by a
   ratio), so the eye approaches the anchor geometrically and never reaches
   or passes it.
2. The distance clamps at `minDistance`, and the anchored target stops
   moving with it ("once distance pins at a clamp the target stops moving
   too"). The zoom goes dead.

The demo keeps that hand-written code as a stopgap, marked to be replaced
by whatever lands here.

## Terms

Three different operations, often all called "zoom":

- **Dolly** - the eye moves toward/away from the target; the distance
  changes. `OrbitCamera`'s zoom is this. Bounded by the target.
- **Zoom (lens)** - the field of view changes; nothing moves.
- **Push / fly-through / truck forward** - eye and target move forward
  together; the distance is constant, so there is no bound and the camera
  passes through the model. **This is what the symptom asks for.**

And two modifiers:

- **Zoom-to-cursor** / zoom-to-point / dolly-to-cursor (anchored zoom) -
  the point under the pointer or pinch centre stays under it while the
  distance changes.
- **Dynamic pivot** / orbit-around-picked-point - rotation re-centres on
  what the camera looks at, so a drag after an anchored zoom or a push does
  not swing the model around a point in empty air.

## What SolidRT has

- **`OrbitCamera`** (`orbit.ts`, `components/orbit-camera.tsx`) - a
  turntable around a fixed target. With `orbitBindings`: drag rotates;
  pinch and wheel dolly toward the target; two-finger drag, Ctrl-drag and
  right-drag pan. Options: speeds, damping, distance/elevation clamps,
  `clampPose`, auto-orbit (`orbitSpeed`), verbs (`glideTo`, `fit`,
  `zoomBy`, `panBy`, `set`).
- Two hooks with nothing behind them: `zoomAnchor(focal, view)` (the app
  maps the focal point to a world point; the dolly pins it) and
  `rotateAnchor(view)` (the app names the pivot when a drag begins). To use
  them an app writes the picking itself: `scene.size()` + `scene.pick()`,
  `scene.unproject()` at the target depth as fallback, a view-centre pick
  for the pivot. `packages/3d/AGENTS.md` says "only the app can build that
  mapping, since it needs the projection" - but `<OrbitCamera>` sits in a
  Scene and reaches it through `SceneContext`, so the component can likely
  build it (check what the context exposes).
- **`FirstPersonCamera`** (`first-person.ts`) - look, walk, rise/sink (fly
  mode); pointer, pad, keyboard. Not an inspection camera, and not driven
  by pinch.
- Nothing for pushing through a model, maps, trackball/arcball, or
  manipulating objects.

## What others offer (from memory - unverified)

**Three.js** - controls are addons, not core:

- `OrbitControls` - orbit/dolly/pan; `zoomToCursor` (off by default),
  `screenSpacePanning`, damping, auto-rotate, configurable touch/mouse
  mappings (`TOUCH.DOLLY_PAN`, ...).
- `MapControls` - pan-first variant of OrbitControls.
- `TrackballControls`, `ArcballControls` - rotation without a fixed up.
- `FlyControls`, `FirstPersonControls`, `PointerLockControls` - free
  movement.
- `TransformControls`, `DragControls` - move objects, not the camera.
- Third-party **`camera-controls`** (yomotsu) - `dollyToCursor`,
  `infinityDolly` (keeps moving forward past `minDistance` by pushing the
  target - the push behaviour above), truck, `fitToBox`/`fitToSphere`,
  `setOrbitPoint`, boundary box, smooth transitions between poses.

**Babylon.js**:

- `ArcRotateCamera` - orbit camera; `zoomToMouseLocation` (off by default),
  inertia, panning sensibility.
- Camera behaviors: auto-rotation, framing, bouncing.
- `FreeCamera`, `UniversalCamera`, `FlyCamera`, `FollowCamera`,
  `VirtualJoysticksCamera`, and a geospatial camera.

**Unity** - no runtime camera controls; the Cinemachine package supplies
orbit and follow rigs. The editor Scene view has zoom-to-cursor and fly
mode, apps do not.

**Godot** - no runtime orbit or fly camera beyond `Camera3D`; addons (e.g.
Phantom Camera). The editor viewport has these, apps do not.

Takeaway so far: Three and Babylon make zoom-to-cursor a single flag, and
`camera-controls` makes the push a single flag. That is the bar.

## Done looks like (to be reshaped after the research)

1. **Push / fly-through on `OrbitCamera`** - an option (name to decide;
   `camera-controls` calls it `infinityDolly`) where a pinch or wheel step
   past a threshold (or always) moves eye and target forward together,
   toward the pinch point, with a step scaled to the subject so speed holds
   inside a model. The fly demo moves to it.
2. **Built-in zoom-to-cursor and dynamic pivot** - one option that does the
   pick/unproject mapping inside the component; `zoomAnchor`/
   `rotateAnchor` stay as overrides. `AGENTS.md` corrected.
3. **Decide the defaults** for touch-first model viewing, and show them in
   `orbitBindings`' docs and the pick example; `orbit-check.ts` covers the
   new behaviour.
4. **The rest of the gap as separate, demand-gated items**, from the
   research: map control, trackball/arcball, fit-to-bounds, orbit-point
   verb, boundary box as a stock `clampPose`, double-tap-to-focus, object
   manipulation controls.
