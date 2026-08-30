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
either way. The spatial-index stage therefore WAITS on that decision
rather than being built here.

## Findings

Stage A landed 2026-08-24 (uncommitted): `packages/2d/src/tiles.ts`
(`createTileLayer(cols, rows, tileW, tileH, atlas)`), `<TileLayer>` in
components.tsx, the sprite shaders factored into `shaders.ts` (shared
verbatim - the vertex stage's uCamera doubles as the chunk-rect bake
mechanism stage B needs), example `examples/tiles.tsx`. Verified live via
the control API: tree shape, two viewport snapshots showing pan + zoom +
tile edits.

- The tile-grid open question resolved grid-first: `setTile(col, row,
  frame | null)` with FIXED record slots (record `row * cols + col`,
  empty = zero-size quad, instance count constant `cols * rows`). No
  draw-order maintenance, no growth, no "records you bake" API - that
  vocabulary stays the sprite layer's.
- Bake = lease publish + `renderTarget()` from the microtask flush; one
  bake at creation so an untouched layer composites as its clear color,
  never undefined target contents.
- Camera is a transform on the composited leaf (`<view x y scale>` around
  origin 0,0: `x = -camX * zoom`, `scale = zoom`), not a second render
  pass - `<texture>` takes no TransformProps, hence the view wrapper. The
  leaf is world-sized; the app clips. Same numbers as the sprite-layer
  camera, so one signal drives both.
- Tile-layer zoom scales the BAKED texture at composite (the sprite layer
  re-samples the atlas in-shader), so pixel art needs `filter: "nearest"`
  on the tile layer's own sampler on top of the atlas's - two different
  samplers.
- Records are the full 13-float sprite layout with rot/tint at defaults:
  52 bytes per cell CPU + GPU (10k tiles = ~1 MB total). Accepted for
  shader reuse; a lean 8-float tile record needs its own vertex stage and
  was not worth it at stage A.

Stage B1 landed 2026-08-24 (uncommitted): chunking inside the same
`createTileLayer` API, plus the camera gaining rotation + pivot
(`TileCamera`). Verified live: 128x128 world (6144px, past maxTextureSize),
129 of 169 chunks resident, snapshots showing rotation about a
bottom-of-viewport pivot with seamless chunk boundaries.

- Per-chunk BUFFERS, not the sketched shared records buffer: GLES 3.0 has
  no base-instance draw (`DrawRange` carries no firstInstance), so a
  shared buffer would vertex-process the whole world per chunk bake. Each
  chunk is a small stage-A layer - own records, own instance buffer, own
  manual target - with records in WORLD coordinates and `uCamera` at the
  chunk's pixel origin; a chunk bake costs exactly its own tiles.
- Lazy = allocate-on-first-content: an empty chunk is nothing (no records,
  no texture), so sparse worlds are bounded by content with zero camera
  coupling. Clearing a cell in an unallocated chunk is a no-op, not an
  allocation. `clearColor` became per-chunk - never-written regions render
  nothing, the ground color belongs behind the layer.
- Composition: the component `<For>`s `d-texture` leaves at chunk world
  rects (chunk growth reaches it through an `onChunk` hook feeding a
  signal), inside the one camera-transformed world view - which is why
  whole-world ROTATION fell out free: rotate the container, chunks are
  rigid inside it. Camera formula: origin at the camera world point,
  rotate + scale there, translate that point onto the screen pivot;
  pivot (0,0) degenerates to stage A's top-left anchoring exactly.
- The stage-A `output` prop was dropped: with chunks there is no single
  texture to hand out; `layer.chunks` + `onChunk` is the compose-yourself
  surface.
- Default chunk edge ~512px of tiles (`chunkTiles` to tune); chunk size
  is validated against maxTextureSize instead of the world size.

Remaining here - stage B2, streaming worlds. Today's contract is bounded
worlds with memory proportional to the TOUCHED area (allocation is
monotonic, nothing evicts): sparse worlds are fine, a fully-painted
1024x1024-tile world at 480px chunks is ~10k chunks x ~920KB of texture -
far past reasonable. Deferred until a real world exceeds texture memory;
when it lands, it is four things that belong together, not just eviction:

1. **A view-rect input into the core layer** - the one signal driving
   everything below (the component's camera already knows it; the core
   layer does not).
2. **Residency**: evict far chunks' textures, keep the ~5KB CPU records
   (~180x cheaper than the texture), re-bake on approach. For truly
   unbounded roaming even records can go: an app-provided re-fill callback
   regenerates a chunk's cells on approach (the procedural-world shape)
   instead of retaining them.
3. **Composition pruning**: `<TileLayer>` currently mounts a `d-texture`
   leaf per RESIDENT chunk, unconditionally - fully-clipped leaves are
   cheap but O(resident). The same view rect prunes the `<For>` to
   camera-intersecting chunks.
4. **Unbounded coordinates**: the fixed `cols x rows` grid caps the world
   at creation (the sparse chunk map inside is already shape-ready), and -
   subtler - records store WORLD pixel coordinates in float32, so past a
   few million pixels from origin subpixel precision erodes and tiles
   shimmer against their chunk rects. The fix is chunk-LOCAL record
   coordinates with the existing per-chunk `uCamera` origin doing the
   placement; a record-layout decision that must land WITH the
   unbounded-world work, not after it.

Bitmap-font runs ride the same machinery. The spatial index waits on
[2d-spatial-citizenship](2d-spatial-citizenship.md), per above. The
rotating-camera parity gap for the LIVE layer is filed as
[2d-sprite-camera-rotation](2d-sprite-camera-rotation.md).

Restating how hard the B2 bound binds, from a demo that leaned on it: two
worlds of 1.18M cells sat at ~260 MB across 152 chunk textures, memory
proportional to WRITTEN area and monotonic, so world size in practice is
capped by content, not by the grid. The chunking removes the
`maxTextureSize` ceiling (a 24,576 px world renders fine) and replaces it
with a memory ceiling that has no back pressure and no diagnostic - at
minimum, `get_gpu_resources` should make the per-layer total legible
before eviction exists. The oversample side of that memory is bounded
since `maxOversample` (total is resident chunks x n squared; the window
texel budget stays per target by design - a layer-total budget would
shrink quality as content fills in, which is worse than a cap).
