---
title: Layer-level pointer events for empty space on the sprite layer
description: SpriteLayer's built-in handlers only ever deliver to sprites, so any app that combines per-sprite interaction with pan/zoom on empty space must abandon the layer's dispatch and re-implement pick, capture, tap-slop and hover on its own leaf - give the layer a miss path.
created: 2026-09-02
---

# Layer-level pointer events for empty space on the sprite layer

## Symptom

Every canvas-shaped app wants the same split: pointer events on a sprite
interact with it, pointer events on empty space drive the camera (drag
pans, pinch zooms) or a marquee. The sprite layer's dispatch
(`spriteDispatch` in packages/2d/src/layer.ts, exposed as
`layer.handlers`) has no miss path - an event over empty space is
swallowed, so the layer-level gesture has nowhere to attach. The
documented workaround is "own handlers on the `<texture>` leaf +
unprojectCamera", but taking the leaf's handlers yourself means the
built-in per-sprite dispatch never runs: the app re-implements picking,
per-pointer capture, tap-vs-drag slop and hover pairing by hand.

Both 2d demos hit this. Starlings only needed layer-level gestures, so
the workaround was cheap. Relay (packages/2d/demos/src/relay.tsx) needs
both sides and carries a full hand-rolled dispatch: pickNode over
`layer.pick`, its own pointer map, its own pinch bookkeeping, its own
hover tracking - roughly a hundred lines every canvas app will paste.

## Shape

Additive, two candidate spellings:

- Miss handlers on the layer/component: `onBackgroundPointerDown/Move/Up`
  (and wheel) that fire when the hit walk finds no sprite. The built-in
  leaf keeps carrying `layer.handlers`; sprite hits dispatch as today,
  misses go to the background set with the same event shape (layer
  pixels, camera undone).
- Or: let the walk bubble past the root - the layer itself as the
  outermost `currentTarget` after the hit sprite's groups, receiving
  every event (hit or miss), with `event.sprite` null on a miss. This
  matches the DOM/3d mental model (the scene as the last stop) and gives
  "layer-wide move regardless of hit" for free.

Either way the capture rule must extend to the background: a drag that
starts on empty space keeps delivering to the background handlers even
when it later crosses sprites, mirroring per-sprite capture.

Comparison: DOM canvas apps get this from event bubbling to the canvas
element; Godot's Node2D `_unhandled_input` is exactly the miss path;
Unity's EventSystem falls through to camera/controller scripts when no
collider is hit. All three treat "nothing was hit" as a first-class
event, which is what the layer lacks.

## Open questions

- Do enter/leave pair against the background too (hover left all
  sprites), or stay sprite-only as today?
- Does the record layer get the same surface (it shares spriteDispatch)?
- Should tap-vs-drag slop stay app code, or ride along here (see the
  camera-controller item, which would consume these events)?
