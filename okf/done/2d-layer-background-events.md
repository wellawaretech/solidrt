---
title: The sprite layer is the root of its pointer walk
description: SpriteLayer's dispatch delivered only to sprites, so any app combining per-sprite interaction with pan/zoom on empty space re-implemented pick, capture, tap slop and hover on its own leaf; the layer is now the last stop of every event (DOM's container, r3f's onPointerMissed, Godot's _unhandled_input), with claiming by stopPropagation, capture to the root, wheel, and synthesized taps.
created: 2026-09-02
completed: 2026-09-06
---

# The sprite layer is the root of its pointer walk

## Symptom

Every canvas-shaped app wants the same split: pointer events on a sprite
interact with it, pointer events on empty space drive the camera (drag
pans, pinch zooms) or a marquee. The sprite layer's dispatch had no miss
path - an event over empty space was swallowed, so the layer-level
gesture had nowhere to attach. The documented workaround was "own
handlers on the `<texture>` leaf + unprojectCamera", but taking the
leaf's handlers yourself meant the built-in per-sprite dispatch never
ran: the app re-implemented picking, per-pointer capture, tap-vs-drag
slop and hover pairing by hand, a hundred lines per canvas app.

## What landed

The DOM event model one tree deeper, in `packages/2d/src/dispatch.ts`
(factored out of layer.ts, pure, headless-checked by
`checks/dispatch-check.ts`), shared by both layer kinds:

- The layer is the root of the walk: down, move, up and wheel dispatch on
  the hit sprite, bubble through its groups and end at the layer's
  listeners (`layer.listen(...)`, `<SpriteLayer onPointer*/onWheel/onTap>`);
  over empty space the walk is the layer alone, `event.sprite` null.
- Claiming is `stopPropagation`, and a stopped DOWN claims the press:
  that pointer's move, up and tap never reach the root (the chain still
  bubbles). One rule covers "drag the sprite, not the view".
- Capture per pointer to the press target, the root included.
- `onWheel` on sprites, groups and the root, same walk.
- `onTap` synthesized by the dispatch: same-target release within the
  slop (8 window px = core's recognizer slop, so a press is never both a
  tap and a pan), the only pointer down for its whole press, `tapCount`
  for repeats within 300 ms and 20 px on the same target.
- Every event carries `native`, the leaf's element event, for core's
  recognizers.
- `createCamera2d` lost its private tap rule and gained `attach(layer)`
  (a root listener, synchronous push on input) and a reactive `active()`
  (fed by the motion's new `active()`/`onActive` hook); `<Camera2d>` is
  the `<OrbitCamera>` shape over it, running frames only while active.
- `LayerBase` gained `width`/`height`.

Verified headless (14 rule groups in dispatch-check.ts) and live through
the control API on examples/camera.tsx (function face, attach) and
examples/pick.tsx (`<Camera2d>`): sprite tap selects, sprite drag moves
the sprite by the exact world delta with the camera untouched, wheel
glides the zoom, an empty tap deselects and glides, an empty drag pans
with inertia after release, a double tap counts 2; zero missed presents.

## Findings

- The four reference models converge on the same five things, all used
  in practice: the container is the last stop hit or miss (DOM,
  r3f's onPointerMissed, Godot's _unhandled_input, Unity's
  IsPointerOverGameObject gate), claiming is stopPropagation, capture
  goes to the press target including the container, wheel rides the
  same walk, click is synthesized with a slop and a count (Godot has no
  click). A miss-handler set beside the dispatch (the first shape
  considered) is Unity's gate as a callback and needs a second event
  shape; the root-of-the-walk shape needs neither, and it is fewer
  concepts, not more.
- The DOWN decides the press. DOM delivers a claimed pointer's later
  events to the container anyway and every canvas app then checks "did
  the down land on the background" by hand; making the claim stick for
  the press is that practice as the contract, and it is what keeps a
  camera at the root out of a sprite drag with one stop.
- The root is a LIST (registration order, all run) where a sprite has
  plain fields: the app's own root handling and controls meet there.
  DOM has both forms (on* property and addEventListener); 3d's
  `scene.input.add` is the same list.
- A tap belongs to the dispatch, not the camera: no engine's camera
  control has one (Godot Camera2D, Three MapControls, Cinemachine), and
  the dispatch is the only place that knows down and up targets.
- The recognizer delivers one delta per FRAME: a synthetic drag under a
  frozen clock pans nothing until frames step (the sprite drag, which
  is plain dispatch, moves immediately). A world-bounded camera at the
  fit zoom or at a clamp corner also pans nothing in the clamped
  direction - both read as "drag does nothing" in a probe.
- Without a `world`, the controller's default pose puts world 0,0 at
  the pivot (the viewport center): a fill layer wanting world = screen
  at rest takes `pivot: { x: 0, y: 0 }`.
- Solid 2 forbids signal writes inside an owned scope: a signal the
  motion writes from its constructor (the camera created in a component
  body) needs `{ ownedWrite: true }`, as core's audio state does.
- `native.button` is set on down/up only, so a sprite drag gates its
  moves on a pressed flag from down to up; the root's move over empty
  space is the hover-on-background signal.

## Not done here

The 3d package has the same gaps (no wheel, no click, and its mesh
channel and `SceneInput` never coordinate, so a mesh drag also orbits):
[3d-scene-pointer-root-walk](../backlog/3d-scene-pointer-root-walk.md).
Deliberately left out with the engine count: group enter/leave (DOM and
Unity yes, r3f and Godot no; sprite-only is parity) and drag
start/drag/end events (Unity only; a sprite drag composes core's
`createPan` through `native`). Live option props on `<Camera2d>` (the
motion reads them once) are additive.
