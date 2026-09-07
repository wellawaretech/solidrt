---
title: The 3d scene is the root of its pointer walk
description: Scene dispatch delivered to meshes only and the pointer feed was spread beside it, with nothing between them, so a mesh drag orbited the camera too, there was no mesh wheel or click, no scene-level miss event, and a view leaf carried no mesh events; the scene (and each view) is now the last stop of every event, with claiming by stopPropagation, capture to the root, wheel, synthesized taps and the feed listening at the root - the 2d layer's model one dimension up.
created: 2026-09-06
completed: 2026-09-07
---

# The 3d scene is the root of its pointer walk

## Symptom

Two channels on the scene leaf that never met: `scene.handlers`
(scene-pointer.ts: nearest hit, bubble mesh -> ancestors, capture,
enter/leave) and the pointer feed's handlers, spread beside them on the
same leaf. Every pointer event reached both, so a drag that started on
a mesh with a drag handler ALSO orbited the camera; an app that wanted
"drag the mesh, not the view" had to disable the control by hand (r3f's
`controls.enabled = false` habit). There was no `onWheel` on meshes (a
wheel over a mesh could not be claimed), no `onTap`/click (every scene
app re-derived down+up-within-slop), no scene-level miss event
(`event.mesh === null` for "tapped the sky", the deselect case), and a
`<View3d>` leaf carried no mesh events at all, since picking was the
scene camera's.

## What landed

The vocabulary [2d-layer-background-events](2d-layer-background-events.md)
settled, one dimension up, so the two packages are one habit
(`packages/3d/src/scene-pointer.ts`, pure, headless-checked by
`checks/dispatch-check.ts`):

- The scene is the root of the walk: down, move, up and wheel dispatch
  on the nearest hit - the struck INSTANCE of an instanced mesh, else the
  mesh - bubble through the ancestors and end at the scene's listeners
  (`scene.listen(...)`, `<Scene onPointer*/onWheel/onTap>`); a miss is
  the scene alone with `event.mesh` null. `NodePointerEvent` is the
  chain's view (mesh non-null), `ScenePointerEvent` the root's.
- `stopPropagation` claims, and a stopped DOWN claims the whole press:
  that pointer's move, up and tap never reach the root.
- Capture per pointer to the press target, the scene included.
- `onWheel` and `onTap` (slop, the alone rule, same-target release per
  instance, `tapCount`) on nodes and the scene; `native` on every event.
- The pointer feed listens at the root: `<Scene pointer>` and `<View3d
  pointer>` bridge it with `feedPointer(root, feed)` (exported for
  imperative scenes), so `<OrbitCamera>` and `<FirstPersonCamera>`
  respect a mesh's claim without changing shape; key routing and
  mouse-look stay on the input map as they were.
- Views are roots of their own walk: `view.pick` (the scene's pixel ray
  over the view's camera and size), `view.handlers`/`handlersFor`/
  `listen`, and the `<View3d>` leaf carries them - a mesh under a minimap
  gets its ordinary handlers, picked where the minimap shows it.

Verified headless (15 rule groups in dispatch-check.ts) and live through
the control API on examples/pick.tsx: a tap pops a mesh with the orbit
still, a drag on the crate slides it over the floor with the orbit pose
untouched, a drag on empty space orbits, a wheel over the ring spins it
and leaves the zoom alone, a wheel elsewhere zooms, a tap on nothing
un-pops through the scene's own handler.

## Findings

- The walk starts at the struck instance, so a target is `{ mesh,
  instance }`: hover pairs per instance (leaving I1 for I2 of the same
  mesh is a leave and an enter), and a tap compares both, so a release on
  another instance of the same mesh is no tap. The root listeners see
  both fields on the event.
- View picking is the scene's pixel ray parameterized by camera and
  target size, nothing more: `pixelRayOf(cam, w, h, x, y)` serves the
  scene and every view, and `scene.raycast` does the rest. Each root runs
  its own dispatch (own capture, hover and press maps), so a press in a
  view and a release over the scene leaf are two roots, as two DOM
  elements would be.
- A node that left the scene mid-press is the inert case (`_scene ===
  null`, where 2d has `layer === null`): its handlers are skipped and the
  walk ends at it (remove() cut its parent), but the root still hears
  the press.
- Core's pointer feed ignores `onPointerLeave`, so a root listener needs
  no leave event and the bridge forwards down/move/up/wheel only.
- The scene object needs the dispatch and the dispatch needs the scene
  as its root: an object-literal getter (`get handlers()`) over a
  dispatch created after the literal resolves the order with no
  placeholder cast.
- Not done on purpose, with the engine count as for 2d: group
  enter/leave (DOM and Unity yes, r3f and Godot no; struck-node-only is
  parity) and drag start/drag/end events (Unity only; a mesh drag
  composes core's `createPan` through `native`, or a screenRay plane
  intersection as pick.tsx does). A view's `project`/`unproject`/
  `screenRay` are additive when a view overlay asks for them.
