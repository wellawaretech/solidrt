---
title: A scene always owns a draw target, so a split-screen app pays for one it never shows
description: createScene allocates its buffer, depth and resolve chain unconditionally, and there is no output={false}, so an app that renders only through <View3d>s carries a full unused scene target; @solidrt/2d took the opposite structure on 09-07 and layers render only through views.
created: 2026-09-11
---

# A scene always owns a draw target

## Symptom

`createScene(width, height, opts)` takes a size positionally and
immediately allocates the scene buffer with its depth and samples, then
builds the resolve chain over it (`packages/3d/src/scene.ts`, the
`createDrawTarget` at the top of the function). `scene.texture` is that
chain's output and there is no way to say the scene has no output of its
own.

So a split-screen app, a picture-in-picture app, or anything whose only
output is two or more `<View3d>`s pays for a whole extra target: an HDR
buffer at the scene size, its depth attachment, the resolve pass, and
the bloom chain if bloom is on. It is cleared and drawn every frame and
never sampled.

`@solidrt/2d` has the opposite structure, and it was settled deliberately
([2d-layer-views](../done/2d-layer-views.md), 09-07): a sprite layer
allocates no target at all, renders only through `layer.createView`, and
`<SpriteLayer output={false}>` is the spelled-out "no view of its own -
the layer shows only through its `<View2d>` children (split-screen: two
side by side, a game with its minimap and nothing else)". The 3d scene
predates that and never got the same separation.

## Three, Unity, Godot

All three separate the world from the thing that renders it, and none of
them makes the world own a target:

- **Three**: a `Scene` is a node tree with no target. The renderer holds
  the target, and `renderer.render(scene, camera)` into
  `setRenderTarget(...)` as many times as you like.
- **Unity**: the scene is the world; a `Camera` owns `targetTexture`. No
  camera means no target and no cost.
- **Godot**: `World3D` holds the world, `Viewport`/`SubViewport` owns the
  texture. A world can be shared by several viewports
  (`world_3d` assignment) precisely for split-screen.

Our 2d matches all three. Our 3d is the outlier, and the split-screen
case is the one every engine's docs use to motivate the separation.

## Done looks like

The 2d contract, in 3d spelling:

- A scene that owns no target. `scene.texture`, `hdrTexture` and
  `depthTexture` are then null, and the target-shaped calls
  (`setSize`, `setResolve`, `setBloom`, `setParams` at scene scope,
  `bakeBackground`) throw the way 2d's view props do with
  `output={false}`. Everything else - the node tree, lights, shadows,
  queries, `createView` - is unchanged, because none of it is the
  target's.
- `<Scene output={false}>`, matching `<SpriteLayer output={false}>`
  exactly, including which props then have nothing to apply to.

The spelling question to settle first is the imperative entry, because
`createScene` takes its size positionally: a size-less overload, an
`output: false` option with the sizes ignored, or a separate
constructor. 2d does not have to answer this (`createSpriteLayer` never
took a size), so there is no precedent to copy.

Two things to check while shaping it, both about what silently assumes a
scene target exists:

- Shadow views follow the SCENE's layer mask by design ("a mesh the
  scene cannot see must not darken it"). With no scene target, that mask
  is still the right filter, but the sentence explaining why stops being
  true and the rule needs restating against the views.
- `scene.pick`, `project`, `unproject`, `screenRay` and `viewProj` all
  read the scene camera, which still exists as state; they keep working,
  but for a view-only app the useful ones are the view's, which is
  [3d-scene-views-additive](3d-scene-views-additive.md).

Additive: the default stays a scene with its own target, so nothing
that exists today changes shape.

## Involves

`packages/3d/src/scene.ts` (the target, resolve chain and every
`setTarget*` call behind one "has an output" branch),
`components/scene.tsx` for the prop and the leaf it stops rendering, and
the AGENTS.md model section, which opens by describing the scene as
compiling to one draw target.
