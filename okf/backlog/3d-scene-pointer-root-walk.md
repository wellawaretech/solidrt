---
title: The 3d scene is not the root of its pointer walk, and mesh drags also orbit
description: Scene dispatch delivers to meshes only and the SceneInput channel to controls only, with nothing between them, so a mesh drag orbits the camera too, there is no mesh wheel or click, and no scene-level miss event; the 2d layer now has the DOM model (the layer as the last stop, claiming by stopPropagation, root capture, wheel, synthesized taps) and the scene should carry the identical vocabulary.
created: 2026-09-06
---

# The 3d scene is not the root of its pointer walk

## Symptom

Two channels on the scene leaf that never meet: `scene.handlers`
(scene-pointer.ts: nearest hit, bubble mesh -> ancestors, capture,
enter/leave) and `scene.input` (`SceneInput.add`, the listener list
`<OrbitCamera>` and `<FirstPersonCamera>` sit on). Every pointer event
reaches both, so a drag that starts on a mesh with a drag handler ALSO
orbits the camera; an app that wants "drag the mesh, not the view" has
to disable the control by hand (r3f's `controls.enabled = false` habit).
There is no `onWheel` on meshes (a wheel over a mesh cannot be claimed),
no `onTap`/click (every scene app re-derives down+up-within-slop), and
no scene-level miss event (`event.mesh === null` for "tapped the sky",
the deselect case).

## Done looks like

The vocabulary [2d-layer-background-events](../done/2d-layer-background-events.md)
settled, one dimension up, so the two packages are one habit:

- The scene is the root of the walk: down/move/up/wheel dispatch on the
  hit mesh, bubble through ancestors and end at scene-level listeners
  (`scene.listen(...)` / `<Scene onPointer*/onWheel/onTap>`); a miss is
  the scene alone with `event.mesh` null.
- `stopPropagation` claims, and a stopped down claims the whole press
  for the root.
- Capture per pointer to the press target, the scene included.
- `onWheel` and `onTap` (slop, alone rule, `tapCount`) on meshes,
  groups and the scene; `native` on every event.
- `SceneInput` becomes the root's listener list (controls attach there
  and so respect a mesh's claim), keeping `handlersFor` and the key
  events it also carries.

Involves: scene-pointer.ts gains the root, the tap tracker and wheel
(the 2d dispatch.ts is the model, near-verbatim); `SceneInput` folds
into it; `<OrbitCamera>`/`<FirstPersonCamera>` unchanged in shape. The
first-person camera's mouse-look (relative mouse) and key routing stay
on the input channel as they are; only pointer events join the walk.
