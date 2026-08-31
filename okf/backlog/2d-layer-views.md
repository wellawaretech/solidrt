---
title: 2D layer views - a second rendering of a layer world (the minimap)
description: A minimap, a zoomed radar strip or a picture-in-picture is common in 2D games, and today the only way to render a layer's world twice is a second layer with every sprite duplicated and double the writes. Mirror the 3d scene.createView contract on the sprite and tile layers - same world, its own camera and size - with the layers bitmask from the 3d view work when the second view needs a different mesh set (markers only).
created: 2026-08-31
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
