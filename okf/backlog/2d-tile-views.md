---
title: The tile layer renders through views, so a minimap minifies the bake correctly
description: A tile layer composites its chunk textures as d-texture leaves, which Impeller samples at level 0 only, so a second composite well below the bake density (a minimap at a tenth of the main view's zoom) aliases; the sprite layer and the 3d scene already render through views (one draw target per camera with its own oversample), and the tile layer should too, sampling mip-chained chunk targets in its own pipeline
created: 2026-10-14
---

# The tile layer renders through views

## Symptom

A tile layer has one bake density (`oversample`, texels per world pixel),
picked as the largest wish among its viewers since
[2d-baked-layers](../done/2d-baked-layers.md) stage B2 closed. Every
composite of the layer (`<TileLayer>`, one per camera) places the chunk
textures as `d-texture` leaves, and Impeller's C API samples a texture
linear or nearest at level 0 (`ImpellerTextureSampling` has no mip mode),
so a composite far below the bake density - a minimap at zoom 0.1 over a
world baked for a zoom 1 main view - minifies ten texels into one at the
leaf and aliases. The example (`packages/2d/examples/tiles.tsx`) shows it
at the minimap's edges; the density pick is right and the leaf is the
limit. The 3d package and the sprite layer have no such limit: a scene
view and a sprite view are DRAW TARGETS with their own oversample, and
their shaders sample mip-chained atlases.

## Done looks like

- **A tile view.** `layer.createView({ width, height, camera?,
  oversample?, clearColor?, label? })`, the contract of
  `LayerBase.createView` (views.ts) one layer kind over: a draw target
  with one entry per resident chunk (added on enter, removed on leave,
  the entries in row-major order so a lower row's overhang draws over the
  row above), a chunk-quad pipeline whose vertex stage places the chunk's
  rect under the view's camera (a uniform write per camera move, rotation
  in-shader) and whose fragment stage samples the chunk's target. Under
  `wrap`, one entry per image of a chunk that intersects the view.
- **Mip-chained chunk targets.** The layer creates its chunk targets with
  `mipmap: true` (a layer option, default on when any view minifies past
  2x, settable) and the resolve tail of every bake regenerates the chain
  (as any content write does), so the view's sampler minifies correctly
  at any zoom; the pool class gains the sampling, since a target's
  sampling is creation-time state.
- **Per-view oversample.** The VIEW target has its own `oversample`
  (texels per view pixel, the sprite view's contract) picked from its own
  on-screen scale; the bake density stays the layer's one number, the
  largest of the views' wishes in world terms.
- **The component.** `<TileLayer>` owns a view like `<SpriteLayer>` owns
  one, shown as one `<texture>` leaf carrying the camera; `<TileLayer
  layer>` adds a view of an existing layer (the minimap). The leaf
  composite (the `<For>` of `d-texture` leaves) goes; nothing else in the
  component's contract changes.
- **Verification.** The example's minimap crisp at zoom 0.1 (a readback
  of the view target against a CPU downscale of the bake), the seam and
  overhang tests over the view target instead of the painted composite,
  the resident count unchanged.

## Involves

`packages/2d/src/tiles.ts` (views, entries on enter and leave, the
mipmap option, the pool class), a chunk-quad pipeline beside `glsl.ts`,
`packages/2d/src/components/tile-layer.tsx`, the tests in
`tests/tiles-residency.test.tsx`, `packages/2d/AGENTS.md` (Scrolling and
the camera, the component props).
