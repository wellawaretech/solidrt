---
title: 2d camera conventions - pose, bounds, easing, input, and how to verify one
description: The decisions behind createCamera2d, each against Godot's Camera2D, Unity's Cinemachine and Three's MapControls (plus the map/whiteboard libraries for the canvas half), and the traps met building and verifying it.
created: 2026-09-06
---

# 2d camera conventions

Distilled from [2d-camera-controller](../done/2d-camera-controller.md).
True regardless of that item.

## Two camera species, one control

Godot's Camera2D and Cinemachine are FOLLOW cameras: a target, limits,
damping, a dead zone. Three's MapControls, Leaflet, MapLibre and tldraw
are PAN-ZOOM controls: bounds, zoom about the cursor, inertia, fit. A 2d
package serving games and canvas apps needs both, and they share the
pose, the bounds and the easing, so one control carries both. A shape
derived from canvas apps alone misses the follow camera entirely.

| Capability | Godot Camera2D | Cinemachine | Three MapControls | Maps, whiteboards |
|---|---|---|---|---|
| World bounds | `limit_*`; left/top wins when world < view | Confiner2D; centers on a skeleton when view > shape | none | Leaflet `maxBounds`, tldraw `contain` + origin |
| Zoom about cursor | user code | user code | `zoomToCursor` | `zoomAround` |
| Smoothing | naive lerp * dt | damping time | `enableDamping` + `update(dt)` | `easeTo` duration + easing |
| Follow target | drag margins, anchor mode | dead zone, soft zone, lookahead | target only | n/a |
| Input ownership | none | InputAxisController | controls attach to the element | built in |
| Inertia on release | none | n/a | damping doubles as it | standard |
| Rotation | yes, limits ignore it | dutch | no | no |
| Fit to bounds | no | Group Framing | no | `fitBounds`, `zoomToFit` |

## Decisions

- **Pose = world point at a viewport pivot**, the pivot a fraction of the
  viewport defaulting to the center (Godot's drag-center anchor,
  Cinemachine's screen position, a map's center). So `x/y` reads as the
  view center, `glideTo` and `follow` land their point there, and rotation
  turns about it. A top-left pivot is the plain scrolling camera. The
  layers' `CameraUpdate` already carried the pivot; the control always
  writes all six fields so the layer never keeps a stale one.
- **Bounds contain the view and center the short axis.** When the view is
  wider than the world on an axis, that axis centers (Cinemachine's
  confiner skeleton, tldraw's contain origin); Godot's left-wins is the
  worse behavior. Consequence every app must know: at exact fit zoom,
  panning is a no-op, because there is nothing to pan.
- **Limits ignore rotation** (Godot's rule): the clamp runs on the
  unrotated view rect. Clamping a rotated view against axis-aligned bounds
  has no good answer and no engine attempts it.
- **Every ease is exponential and frame-rate independent**, `1 - exp(-rate
  * dt)`. Godot's `position_smoothing_speed * delta` lerp is the naive
  form; Three had to add `update(deltaTime)` to fix its per-frame damping.
  Zoom eases in LOG space so a long glide (0.2 -> 3) reads evenly; in
  linear zoom the zoom-in half races and the zoom-out half crawls.
- **A glide lands exactly** (snap inside a relative zoom epsilon and a
  half-pixel travel) and then writes nothing; a follow that reached its
  target settles the same way. Otherwise a resting camera writes
  `setCamera` every frame forever, which keeps frame demand alive.
- **Inertia needs no clock.** The drag's deltas accumulate between
  `update(dt)` calls, each update folds them into an EMA velocity, and a
  release folds the delta still pending from its own frame with the last
  frame time. Pointer input is frame-batched anyway, so one delta per
  frame is the real cadence. A release while following never flings: the
  follow eases the view back instead.
- **A direct manipulation wins over motion in flight**: a pan, an anchored
  zoom or an x/y/zoom write cancels a glide or fling; a wheel notch
  retargets the pending anchor glide (notches compound, so a fast scroll
  is one long push); a rotation write leaves glides running.
- **maxZoom below the fit zoom wins** and the world floats centered,
  rather than throwing: a tiny world in a huge window is not an error.

## Input conventions

- The control owns input through a `handlers` set spread onto the layer's
  own leaf, over core's `createTransform` (arena arbitration, slop
  swallowed, one delta per frame), exactly like the 3d orbit camera. A
  press with no recognizer engagement is a tap, reported as a world point.
- Anchors use the event's `localX/localY` (the leaf's own pixels = layer
  pixels); the recognizer's deltas are window-logical (`clientX/clientY`),
  the convention ScrollView already follows. So the input leaf must not be
  scaled relative to the window (a designSize fit scales it); translation
  is fine.
- The recognizer's slop is swallowed: the first move that crosses it
  contributes no delta. A synthetic 8-move drag therefore pans 7 moves'
  worth.
- The sprite layer's own dispatch and a camera on the same leaf both act
  on a drag that starts on a sprite; the split needs the layer's miss path
  ([2d-layer-background-events](../backlog/2d-layer-background-events.md)),
  which is why the component form waits.

## Verification traps

- `srt:events` (the `pointerFrame` terminator the transform recognizer
  measures on) exists only under lattice, so nothing importing core's
  recognizers runs headless on the bare flux binary. Keep the motion math
  in a pure module and check that headless; drive the input glue live.
- `POST /clock?step=n` QUEUES frames (`pendingSteps` in the reply); a
  debug read right after it races the steps. Wait for them to drain (a
  blocking `/logs?since=<huge>&wait=<ms>` is a serviceable sleep) or step
  in real time with `scale=1` and read after a wait.
- The MCP bridge resolves the server by the cwd's project; from the repo
  root that is the user's own repo-root server. Verify an example on a
  `--file` server with its own port and curl its control API directly.
- A debug command that returns the control's `camera()` is the whole
  verification surface for a camera: the layer camera is a uniform, so
  nothing in the tree shows it.
