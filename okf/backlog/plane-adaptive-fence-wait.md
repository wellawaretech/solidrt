---
title: Make the fence wait over a video plane adaptive
description: An app animating over a playing video plane runs at 16 fps because every window present waits for its GPU work while a plane exists; wait only for isolated presents, and not while the plane is paused.
created: 2026-09-12
---

# Make the fence wait over a video plane adaptive

An app that animates over fullscreen video on the Android plane runs at
about 16 frames a second while the video itself stays perfect. The same
transition with no video runs at 51 a second on both test devices.

While a plane exists, `present()` in `alloy/src/raster/frame.rs` calls
`finish_gpu_work()` before every swap (gated on `video_plane_active()`).
That wait is what protects the video under a static overlay: a window buffer
that reaches the compositor with GPU work outstanding holds the video frame
sharing its atomic commit. For an animating app it serializes the frame:
draw, wait for the GPU, then a swap that blocks for the rest of the period,
so no frame overlaps its GPU work with the next frame's build.

Measured on the Samsung SM-T500, 25 fps video on a 60 Hz panel (findings in
[[android-video-punch-through]]):

| over a plane | video on pattern | app fps |
|---|---|---|
| transition, fence wait on | 100% | 16 |
| continuous demand, no fence wait | 93% | 25 |
| overlay 5 a second, fence wait on | 100% | 5 |
| overlay 5 a second, no fence wait | 60% | 5 |
| transition, no plane at all | n/a | 51 |

What costs the video a frame is an isolated present, not presenting often:
at 25 presents a second none is isolated and the GPU stays awake, so its
fence signals promptly and there is nothing to wait for.

## Fix

- Wait only for an isolated present: the previous present was more than
  about one refresh period ago (a named constant, in periods). Skip the wait
  while the app presents every frame.
- Skip it while the plane is paused. Nothing is released beneath the window
  then, so the wait buys nothing; today the gate is the plane existing.

## Done looks like

With the SurfaceFlinger census on the video layer, on the tablet and the TV:
the overlay repainting 5 a second (`examples/video/src/plane.tsx`) stays at
100% on pattern; the transition over the plane
(`examples/video/src/probe/plane_transition.tsx`) is back to about 25 a
second with the video at 93% or better; a paused plane adds no fence wait.
