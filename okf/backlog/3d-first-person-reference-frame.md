---
title: First-person camera: a reference frame
description: FirstPersonCamera's pose is in world space, so inside a model that rotates as a whole the walls drift past a camera that stays still, and rotating the pose by hand every frame fights the control's own glide and clamps; a frame option on the control that keeps the pose in a node's local space.
created: 2026-09-22
---

# First-person camera: a reference frame

Context: `packages/3d/src/first-person.ts` (the control: pose
`{ position, yaw, pitch }` in world space, `moveSpeed`, `boostSpeed`,
`fly`, `clampPosition`, `set`/`glideTo`, pushed through `setCamera`
each update), `components/first-person-camera.tsx`. The orbit-side gap
(push through a model, zoom-to-cursor) is
[camera-and-controls-extensions](camera-and-controls-extensions.md);
this item is the one first-person ask left and is not research-gated.
(The boost action filed with it landed 2026-09-22: `boost` as an axis
on the control and `firstPersonActions`, `boostSpeed` scaling the move
and rise rates, Shift and the left stick press in
`firstPersonBindings`.)

Symptom: inside a model that rotates as a whole, the control's pose is
in world space, so the walls drift past a camera that stays still.
Rotating the pose by hand with `cam.set()` every frame fights the
control's own glide and clamps.

Done looks like:

- `frame?: SceneNode` on the options and the component: the pose lives
  in the frame's local space. Each update composes eye and look
  direction through the frame's world matrix (`worldMatrix` from
  `flux:spatial` on `frame._node`, one read per frame, no JS walk) and
  pushes the world result to `setCamera`; walking projects onto the
  frame's ground plane and `rise` moves along the frame's up, so a
  tilted or rotating frame keeps "forward" and "up" meaning the room's.
- `clampPosition` runs in frame space (a level's bounds are authored
  there); `eye()`/`forward()` report world, `pose()`/`set()`/`glideTo()`
  frame space. Changing `frame` on a live control re-expresses the pose
  in the new frame so the eye does not jump.
- Pointer look is unaffected: yaw and pitch are frame-relative angles.
- A probe: a room group spinning at a known rate with the control
  framed to it; `/snapshot` twice a quarter turn apart shows the same
  wall in front. AGENTS.md's first-person paragraph gains the option.
