---
title: Baked layers and tilemaps for @solidrt/2d
description: Static 2D bulk (tile worlds, backgrounds) rendered once into a texture and drawn as ONE quad, with incremental re-bake - the primitive-count answer for tiled GPUs
created: 2026-08-19
---

# Baked layers and tilemaps for @solidrt/2d

Stage 2 of the 2D extension (okf/plans/2d-extension.md). The sprite layer
batches draw CALLS, but on tiled GPUs the budget is primitive count
(@solidrt/core agents/performance.md, "Where GPU work stops being free": 20k
verts -> 80ms on a 2017 Android TV, while 9x the fill was free) - so a
100x100 tile world must not be 10,000 quads per frame. Baked, it is one
texture drawn as one quad, and scrolling it is a camera write.

Shape to explore:

- A `TileLayer` (or `BakedLayer`) that rasterizes its content into a texture
  once, then draws that texture as a single quad through the same camera as
  the sprite layer. Content changes re-bake incrementally (chunked: re-bake
  only dirty chunks, e.g. 256px tiles of the baked texture).
- Baking path options: a one-off instanced draw into a `render: "manual"`
  target (the sprite pipeline already draws atlas cells - bake = render the
  tile records once, then stop), vs CPU compositing + uploadTexture. The
  manual-target route reuses everything and keeps pixels on the GPU.
- Chunking doubles as the world-larger-than-viewport answer: bake chunks
  lazily as the camera approaches, drop far ones (texture memory bounds).
- Tier-1 interop note: `repaintBoundary="snapshot"` already approximates a
  baked layer for rendertree content; this item is the GPU-layer analog that
  shares the sprite layer's atlas and camera.
- Bitmap fonts ride the same machinery (glyphs are atlas cells; a text run
  bakes or draws as sprite records) - keep it in scope here rather than as
  its own system.

Open questions: chunk size heuristics, whether tile data gets a first-class
grid API (`setTile(x, y, frame)`) or stays "records you bake", and how far
z-interleaving between baked and live layers needs to go (today: separate
`<texture>` leaves, tree order decides).

## The spatial index belongs with this

Two things in the live sprite layer are O(population) and only start hurting
when the world exceeds the viewport - which is exactly the case this item
exists for:

- every live record draws whether or not the camera can see it (the flush
  publishes the whole live prefix and `instanceCount` covers all of it)
- `pick` walks every record, topmost first, on every pointer move

One uniform grid over the records answers both: a camera-rect query gives the
visible set to compact into the instance buffer, and a point query gives
picking its candidates. Build it here rather than as its own item - alone it
optimizes a case nobody has hit, and the chunking this item needs is the same
spatial decomposition.

If [2d-spatial-citizenship](2d-spatial-citizenship.md) lands, the core BVH
covers culling and picking for the live layer and this JS grid is not
needed; the chunking above (static arithmetic over tiles) is unaffected
either way.
