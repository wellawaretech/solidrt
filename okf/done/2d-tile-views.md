---
title: The tile layer renders through views, so a minimap minifies the bake correctly
description: A tile layer composites its chunk textures as d-texture leaves, which Impeller samples at level 0 only, so a second composite well below the bake density (a minimap at a tenth of the main view's zoom) aliases; the sprite layer and the 3d scene already render through views (one draw target per camera with its own oversample), and the tile layer should too, sampling mip-chained chunk targets in its own pipeline
created: 2026-10-14
completed: 2026-10-16
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

## Outcome

Landed 2026-10-16 as shaped, with these decisions on top:

- The view shell (target, size, oversample, camera, dispatch) moved out
  of the sprite layer's views into `createViewShell` in views.ts, so
  both kinds return the one `ViewHandle` type and `createCamera2d`,
  `feedPointer` and `<Camera2d>` work on a tile view unchanged. A tile
  view's `pick` is empty; its listeners see every event in world px.
- `addViewer`/`TileViewer` are gone: the view IS the viewer (its camera
  pushed to the gate on every camera, size or oversample write, its wish
  for the bake `oversample * zoom`). The bake density is `density` /
  `maxDensity` / `setDensity` (the word `oversample` means texels per
  view pixel everywhere a view exists); the layer's `maxPixels` and
  `filter` options went with the leaf composite.
- Mip chains are on by default (`mipmap: false` opts out), not switched
  on by a minifying view: sampling is creation-time state on a target,
  so the switch would have meant re-baking the world when a minimap
  first appeared. The chain weighs a third more against `maxTexels`.
- One chunk-quad pipeline per process (tile-pipeline.ts); the camera
  reaches the stage split by chunk (`uOrigin` + the remainder) so a
  world 10^7 px out stays exact in float32, which also tightened the
  addressable world to 2^24 chunks per side (the float32 integer limit).
- Under `wrap` a chunk has one entry per image inside the view's world
  box, in row-major world order (exact painter's order at any view
  size); the image set depends on the camera through four integers (the
  chunk-index range the box covers), compared on every camera write and
  rebuilt on a change. The in-shader image window considered first was
  dropped: it could not keep the seam's overhang order once a view
  showed the world repeated.
- `<TileLayer>` became `<SpriteLayer>`'s shape: the view props (fill or
  fixed, camera, clearColor, oversample knobs, viewRef, output, events,
  pointer, the on* handlers) and `<Camera2d>` children.

Verified: the residency suite over the view target (seam, overhang, a
checker world at 1/16 zoom resolving to mid-gray through the chain and
to whole cells without it), the whole 2d suite (148), camera-probe
CAMERA-OK, and the tiles example through the control API: both views
drawing every resident chunk, the minimap at a tenth of the zoom clean.

Tablet (SM-T500, 2000x1200 at display scale 1.5, the tiles example,
2026-10-16), five-second windows once the startup bakes settled, main
view at the auto oversample 2 (2666x1600):

| path | fps | missed presents / 5 s | GPU ms per frame | CPU p95 ms |
|---|---|---|---|---|
| views (this item) | 60 | 7 to 11 | 15.2 | 2.5 to 3.4 |
| old leaf composite (A/B by stash) | 55 to 58 | 21 to 27 | 10.6 to 11.4 | 10.4 to 11.2 |

The hitches an observer sees (50 to 67 ms gaps every one to three
seconds, on both paths; 100 ms on the old one) line up with chunk
enters (about six per second as the ship flies): a bake landing on a
frame near the budget. The view path spends more GPU (the 4.3 MP
intermediate target, 1.8x the screen's pixels because the 1.5x display
scale rounds up to oversample 2) and far less CPU (no per-frame leaf
re-placement); mips cost under 1 ms of the GPU time. At oversample 1
the view path reads 58 fps and 14 ms GPU. The hitches are their
own item: [2d-tile-world-frame-hitches](../backlog/2d-tile-world-frame-hitches.md).
