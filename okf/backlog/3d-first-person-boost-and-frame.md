---
title: First-person camera: boost action and a reference frame
description: FirstPersonCamera has no sprint, so a boost is a keyboard-only moveSpeed swap the pad never gets, and it cannot ride a moving node, so inside a rotating model the walls drift past a camera that stays still in the world; a boost action in the bindings and a frame option on the control.
created: 2026-09-22
---

# First-person camera: boost action and a reference frame

Context: `packages/3d/src/first-person.ts` (the control: pose
`{ position, yaw, pitch }` in world space, `moveSpeed`, `fly`,
`clampPosition`, `set`/`glideTo`, pushed through `setCamera` each
update), `input.ts` (`firstPersonActions = { look, move, rise }` and
`firstPersonBindings` over the input map's `button`/`axis`/`vec2`
action kinds), `components/first-person-camera.tsx`. The orbit-side gap
(push through a model, zoom-to-cursor) is
[camera-and-controls-extensions](camera-and-controls-extensions.md);
this item is the two first-person asks and is not research-gated.

## 1. A boost action

Symptom: sprint was done with the live `moveSpeed` prop and a Shift
signal, which a pad cannot press.

Done looks like:

- `firstPersonActions` gains `boost: "button"`; `firstPersonBindings`
  binds Shift and a pad button (the left stick press, the usual sprint
  on pads; a shoulder as a second binding).
- `boostSpeed?: number` on the options (a multiplier on `moveSpeed`,
  default 2), applied while the action is held, to walk and fly alike,
  in the same frame as the move it scales.
- The component prop, `first-person-check.ts` (a held boost doubles the
  step), AGENTS.md's first-person paragraph.

## 2. A reference frame

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
