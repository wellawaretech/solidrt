---
title: Screen-space sizing for sprites (min-px floors, constant-size markers)
description: Markers that must stay legible at any zoom (selection rings, map pins, traffic dots) have no shader-side answer, so apps rewrite w/h from JS on every camera change - per frame for record sprites - even though the camera is already a uniform in the vertex stage.
created: 2026-09-02
---

# Screen-space sizing for sprites (min-px floors, constant-size markers)

## Symptom

A zoomable world always carries a few sprites whose size is about the
SCREEN, not the world: a selection ring that must stay findable at the
zoomed-out overview, a map pin, a drag handle, a traffic dot that should
never fall under two device pixels. Today size is world pixels scaled by
the camera, full stop, so the app claws the floor back from JS:

- Relay resizes its selection/hover rings on every camera apply
  (`RING_MIN_PX / cam.zoom` in packages/2d/demos/src/relay.tsx) - fine,
  two setSprite calls, but easy to forget and wrong for one frame if
  applied out of order with the camera write.
- Relay's pulses are record sprites, so the floor means rewriting w/h
  for every live pulse every frame (`PULSE_MIN_PX / cam.zoom`), turning
  a positions-only write loop into a positions-plus-size loop.

The vertex stage already has the camera (`uCamera`, zoom in .z/.w), so
the divide the app performs per sprite per frame is one shader
instruction away from free.

## Shape

Per-sprite, opt-in, both layer kinds share the vertex stage so one
mechanism covers both:

- Minimal: a min-screen-px field - drawn size =
  `max(iSize * zoom, minPx)` (per axis, preserving aspect). Covers every
  case met so far; zero cost when 0.
- Fuller: a size-space flag (world | screen), where screen-space sprites
  ignore zoom entirely - the constant-size gizmo. The min-px field is
  the blend of the two and may be the better single knob.

Record layout note: the 13-float record has no free slot, so the record
layer either grows the record (breaking FLOATS_PER_SPRITE consumers) or
takes a per-layer uniform floor (`layer.setMinScreenPx(n)`) - the
per-layer spelling would have served both Relay uses and avoids the
layout change. The node layer's style record has room for a per-sprite
field if wanted (style floats are not slot-constrained the same way).

Picking must agree with drawing: a min-px-floored sprite that draws 15px
should hit-test at 15px, so pointInSprite/pick need the same clamp -
that is the real cost of the feature, and the reason it belongs in the
package rather than in app code (Relay's rings pick at their world size
today, subtly wrong at overview zoom).

Comparison: Three ships this as `Sprite` (screen-facing) plus
`sizeAttenuation: false` on sprites/points - the exact world/screen
split; Unity's gizmos and canvas-space UI are constant-size by
definition and world markers use billboards with constant-screen-size
scripts; Godot has no built-in and its forums carry the same
divide-by-zoom workaround this item removes.

## Open questions

- Per-sprite field vs per-layer uniform floor - or both, uniform first
  (cheap, no layout change) and per-sprite when a use case demands
  mixing floors in one layer?
- Does a floored sprite's rotation stay world-space (yes, presumably) and
  does the floor apply before or after group scale?
