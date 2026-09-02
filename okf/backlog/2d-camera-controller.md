---
title: A 2d camera controller - fit, clamp, zoom-at-cursor, pinch, glide
description: Every pannable/zoomable app re-derives the same hundred lines of camera math (fit-to-world min zoom, pan clamping, wheel zoom anchored under the cursor with an eased glide, pinch anchoring) - ship it once as a camera controller over the shared CameraUpdate.
created: 2026-09-02
---

# A 2d camera controller - fit, clamp, zoom-at-cursor, pinch, glide

## Symptom

The camera type (`CameraUpdate`) and the projection math are shared, but
everything an app does WITH a camera is app code, and it is the same app
code every time: compute the fit-to-world minimum zoom, clamp the pan so
the view stays inside the world (centering an axis whose view is wider
than the world), zoom about the cursor by re-deriving x/y from an
anchored world point, ease a wheel zoom toward its target so scrolling
reads as one push, keep a pinch's midpoint pinned to its world point.
Starlings hand-rolled all of it; Relay copied starlings' spellings and
constants verbatim because getting the feel right again from scratch is
real work (packages/2d/demos/src/starlings.tsx and relay.tsx carry
near-identical WHEEL_ZOOM / ZOOM_EASE / ZOOM_EPSILON blocks and the same
anchor algebra). Two subtle traps live in this code: the clamp must
center when view >= world, and at exact fit zoom panning is a no-op
(Relay's autopilot deadlocked on that before zooming first).

## Shape

A plain-object controller in @solidrt/2d, no signals, both layer kinds:

    let cam = createCamera2d({
      worldW, worldH,        // clamp bounds; absent = unclamped
      maxZoom,               // min defaults to the fit zoom
      viewport: () => ({ w, h }),
    })
    cam.panBy(dxScreen, dyScreen)
    cam.zoomAt(sx, sy, factor)         // immediate, anchored
    cam.wheel(sx, sy, deltaY)          // eased glide toward the target
    cam.pinch(pointers)                // midpoint-anchored two-finger zoom
    cam.glideTo(wx, wy, zoom)          // programmatic focus move
    cam.tick(dt)                       // advances glides; returns changed
    cam.current                        // the CameraUpdate to hand setCamera

The app stays the owner of when to apply: read `cam.current` after
`tick`/mutations and call `layer.setCamera` itself (flush stays with the
phase owner). Constants (glide easing, epsilon) become options with the
demo-proven defaults.

Comparison: Godot ships Camera2D with limits, zoom and position
smoothing; Unity leans on Cinemachine for exactly this layer; Three
ships OrbitControls/MapControls beside the camera rather than making
every app re-derive them. The controls-beside-the-camera split is the
right precedent: the camera type stays dumb, the controller is optional
and additive.

## Open questions

- Rotation support from day one (the shared type has it; clamping a
  rotated view against world bounds is the hard part) or explicitly out
  of scope initially?
- Inertia/fling on release: include (ScrollView's fluent-motion work may
  share the curve) or leave to a later additive option?
- Does it consume the layer background events directly (see
  2d-layer-background-events.md) as an optional `attach(layer)`, or stay
  purely functional with the app forwarding events?
