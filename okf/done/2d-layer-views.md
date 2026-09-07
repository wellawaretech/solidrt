---
title: 2D layer views - a second rendering of a layer world (the minimap)
description: A minimap, a zoomed radar strip or a picture-in-picture is common in 2D games, and today the only way to render a layer's world twice is a second layer with every sprite duplicated and double the writes. Mirror the 3d scene.createView contract on the sprite and tile layers - same world, its own camera and size - with the layers bitmask from the 3d view work when the second view needs a different mesh set (markers only).
created: 2026-08-31
completed: 2026-09-07
---

# 2D layer views - a second rendering of a layer world (the minimap)

## Symptom

A 2D world rendered at two cameras at once - a minimap in the corner, a
radar strip, a zoomed inset over a tile map - has no path. `@solidrt/2d`
layers own exactly one target and one camera, so the app builds a SECOND
sprite layer over the same atlas and mirrors every addSprite/setSprite
into it: double the sprites, double the per-frame writes, and the two
copies drift the moment one write is missed. The 3d package had the same
gap until `scene.createView` (roadmap item 15's multi-view shape); the
minimap is the 2d spelling of the same need.

## Shape: the 3d view contract, one dimension down

`layer.createView({ width, height, camera?, ... })` on the sprite layer,
mirroring `scene.createView`'s surface (`setCamera` in the layer's shared
`CameraUpdate` type, `setSize`, `setParams`, `dispose`; disposed with the
layer). The cost model is better than 3d's:

- The node-backed layer's sprites live in one instance buffer driven by
  the spatial arena; the camera is a shared-params write (`uCamera` /
  `uCameraRot`). A view is one more target drawing THE SAME instance
  buffer with its own shared params - no per-sprite mirroring, no extra
  arena sinks, no per-frame JS. The records layer is the same story
  (one buffer, a second target).
- The tile layer composites baked chunk targets; a view is a second
  composite of the SAME chunks under its own camera - no re-bake, which
  is exactly the property scrolling already has.

Selection (markers-only minimap, hiding a HUD plane from the inset)
reuses the vocabulary settled in
[3d-view-mesh-selection](3d-view-mesh-selection.md): a `layers` bitmask
on the sprite (default 1) and a mask on the layer and each view. Same
names, same semantics, one API shape across 2d and 3d - the "layer"
noun collision with SpriteLayer/TileLayer is the CanvasLayer-vs-layers
coexistence Godot already lives with. Demand-gate the bitmask half
independently of the view half: a plain minimap needs no selection.

A zoomed-far-out minimap of a huge tile world may be cheaper as a scaled
draw of the chunk targets than as a real second composite at fractional
zoom; that is an implementation choice inside the view, not API.

## Done looks like

A tile-map game shows a corner minimap (one view, its own camera at
1/16 zoom) with the player marker sprites on it, at zero per-frame JS
beyond the camera writes; the sprite-layer example gains the inset.

## Not in this item

Post effects on views, per-view tint (layer tint already fans out;
follow the 3d view-owned-params rule if a consumer wants a per-view
override), camera-driven chunk residency
([2d-baked-layers](2d-baked-layers.md)).

## Done

Landed 2026-09-07 on the sprite layer and the record layer:
`layer.createView({ width, height, camera?, oversample?, clearColor?,
label? })` returning a `ViewHandle` (the layer's viewport contract - texture,
size, oversample, camera, project/unproject, listen/handlers/handlersFor,
dispose - minus the sprites, which stay the layer's), in
[views.ts](../../packages/2d/src/views.ts). The 3d handle type `View` was
renamed `ViewHandle` in scene.ts, so both packages read the same.

What it took, and why: `createPipelineTexture` hides its pipeline, so both
layers now spell the draw out - `linkProgram` + `createRenderPipeline` once
per layer, the layer's own target a `createDrawTarget` with one `addDraw`
entry carrying what the fused call carried (quad, instance buffers,
`instanceOrder`, blend, atlas, clear). A view is one more draw target with
one entry over the same pipeline and buffers and NO order of its own: the
core gathers the buffers themselves into key order at publish (one ordered
entry per buffer is the registry's rule, and a view entry declares none),
so the view reads them sorted for free. Growth fans `setDrawBuffers` out to
every view entry after the layer's own `setDraw`, the instance count and the
tint likewise; the camera and viewport are the view's own target params.
The six camera fields and the `uCamera`/`uCameraRot` write that `layer.ts`
and `records.ts` each carried are now `defaultCamera`/`applyCamera`/
`cameraParams` in camera.ts (pure), which a view is the third user of.
Pointer events on a view leaf are the layer's `spriteDispatch` with the
view's camera, over the layer's pick, the view as root - so
`createCamera2d(view).attach(view)` and sprite handlers under a minimap
work unchanged; only the root type widened.

Verified: the refactored main target is byte-identical to the pre-change
capture on examples/pick.tsx (`/texture` PNG, `cmp`); examples/parity.tsx
and examples/camera-probe.tsx end in PARITY-OK / CAMERA-OK on the new
path; examples/views.tsx (the minimap over a 2400x1600 world) shows 301
instances in the view entry, a synthetic tap on the minimap glides the
main camera to the tapped world point exactly, and a tap on a sprite
through the minimap selects it.

Not done here, on purpose - the tile-map minimap with marker selection
this note's "done looks like" named needs tile-layer views and the
`layers` bitmask, both additive and filed as
[2d-layer-views-additive](../backlog/2d-layer-views-additive.md).

Later the same day the layer's OWN target went: a layer renders nothing
by itself and shows only through its views (`createSpriteLayer(atlas,
opts)` / `createRecordLayer(atlas, opts)` take no size; the
`<SpriteLayer>`'s built-in leaf is one view, `<SpriteLayer
output={false}>` has none and shows through `<View2d>` children -
examples/split-screen.tsx). This is the Unity/Godot model (a scene
renders only through Cameras, a World2D only through Viewports), and it
removed the duplicated draw path and viewport methods `layer.ts` and
`records.ts` had carried: `createSpritePipeline` (shaders.ts) owns quad,
program and pipeline, views.ts owns every target. Key order lives on the
first live view's entry and re-homes to the next when that view goes.
`useSpriteLayer()` reports the nearest view as `viewport`; `useScene()`
in 3d renamed its `camera` member to `viewport` to match.
