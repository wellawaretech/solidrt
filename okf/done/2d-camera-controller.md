---
title: A 2d camera controller - fit, clamp, zoom-at-cursor, pinch, glide
description: Every pannable/zoomable app re-derived the same hundred lines of camera math (fit-to-world min zoom, pan clamping, wheel zoom anchored under the cursor with an eased glide, pinch anchoring); createCamera2d in @solidrt/2d ships it once, with follow, inertia and rotation, in the 3d orbit camera's shape.
created: 2026-09-02
---

# A 2d camera controller - fit, clamp, zoom-at-cursor, pinch, glide

## Symptom, as shaped

The camera type (`CameraUpdate`) and the projection math were shared, but
everything an app did WITH a camera was app code, and it was the same app
code every time: compute the fit-to-world minimum zoom, clamp the pan so
the view stays inside the world (centering an axis whose view is wider
than the world), zoom about the cursor by re-deriving x/y from an
anchored world point, ease a wheel zoom toward its target so scrolling
reads as one push, keep a pinch's midpoint pinned to its world point.
Every app re-derived the same constants and the same anchor algebra, and
two subtle traps lived in that code: the clamp must center when view >=
world, and at exact fit zoom panning is a no-op (an autopilot deadlocked
on that before zooming first).

## What shipped (2026-09-06)

`createCamera2d(layer | layers, options)` in `packages/2d/src/camera2d.ts`
over the pure motion in `camera-motion.ts`; exported from the package,
documented in the package AGENTS.md (camera control bullet), pinned by
`checks/camera2d-check.ts` (headless on flux) and shown live by
`examples/camera.tsx`.

The scope grew from the item's pan-zoom sketch to what Godot's Camera2D,
Cinemachine and Three's MapControls together settle as the 2d camera:
pan with inertia on release, wheel and pinch zoom about the pointer,
eased `glideTo` and `fit(rect, { glide })`, `follow(x, y)` through a dead
zone with damping, rotation about the pivot, and world bounds that
contain the view. The shape mirrors `createOrbitCamera`: the first
argument is anything with the layers' `setCamera` (one layer, several,
or a signal setter for `<TileLayer camera>`), `update(dt)` pushes the pose
when it changed, `handlers` ride core's `createTransform` recognizer, so
the two packages' cameras are one habit. The conventions decided and the
traps found are in [2d-camera-conventions](../notes/2d-camera-conventions.md).

The item's open questions, answered:

- Rotation from day one: yes, immediate writes only (`set({ rotation })`
  leaves a glide running), clamped under Godot's limits-ignore-rotation
  rule. Rotation glides and two-finger twist stay additive.
- Inertia on release: in, from the drag's own per-frame deltas, no clock.
- Input: the controller owns it through `handlers` (Three's and the map
  libraries' model; Godot's "no input" was the non-answer). An
  `attach(layer)` or a `<Camera2d>` component needs the sprite layer's
  miss path first ([2d-layer-background-events](2d-layer-background-events.md)).

Verified: the check on three seeds (contain centering, anchoring under
random pivots and rotations, glides landing exactly and resting, follow
with and without a dead zone, fling distance, the deferred fit, the
validation throws); live through the control API on the example, wheel
notch, drag with release, tap, follow, spin and fit each read back from
a debug command against hand-computed poses, and a 3 s stats window with
no missed presents.

## Not done, on purpose

A `<Camera2d>` component, two-finger twist rotation, rotation glides, a
contain origin other than center, and any change to core: all additive.
