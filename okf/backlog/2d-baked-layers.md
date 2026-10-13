---
title: Baked layers and tilemaps for @solidrt/2d
description: Static 2D bulk (tile worlds, backgrounds) rendered once into a texture and drawn as ONE quad, with incremental re-bake - the primitive-count answer for tiled GPUs
created: 2026-08-19
---

# Baked layers and tilemaps for @solidrt/2d

Stage 2 of the 2D extension (okf/done/2d-extension.md). The sprite layer
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

[2d-spatial-citizenship](../done/2d-spatial-citizenship.md) landed: the
core BVH covers picking for the live layer (`pick` and `overlap` are core
queries now), so the JS grid is not needed. What remains of this section
is the compaction half, folded into stage B2 below as its fifth part.

Decided 2026-09-06, when the 3d frustum gate landed in the spatial core:
that gate flips a DRAW ENTRY's instance count, and a sprite layer is one
entry for the whole population, so wiring it to the 2d camera would cull
the whole layer or nothing. A sprite is a record, not a draw (the design
choice Godot's and Unity's per-sprite culling does not share: their
sprites ARE draws), and an off-screen record costs one degenerate quad in
the vertex stage and no fragments - nothing to win at tens of thousands.
The 2d culling that matters is this item's chunk residency and
composition pruning; record compaction is the sprite-layer form and pays
only at hundreds of thousands of records.

## Findings

Stage A landed 2026-08-24 (uncommitted): `packages/2d/src/tiles.ts`
(`createTileLayer(cols, rows, tileW, tileH, atlas)`), `<TileLayer>` in
components/tile-layer.tsx, the sprite shaders factored into `shaders.ts` (shared
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

## Stage B2: streaming worlds

What remains, and the 2d culling item. Today's contract is bounded worlds
with memory proportional to the TOUCHED area: allocation is monotonic,
nothing evicts, and nothing reports. Two worlds of 1.18M cells from a demo
sat at ~260 MB across 152 chunk textures; a fully painted 1024x1024-tile
world at 480px chunks is ~10k chunks x ~920KB. The chunking removed the
`maxTextureSize` ceiling (a 24,576 px world renders) and replaced it with a
memory ceiling that has no back pressure and no diagnostic. Every tile
engine streams by camera, so this is certain work, not gated on a further
app.

There is no backward compatibility, so stage A's creation form, the
world-sized view composite, the record layout and the `chunks` + `onChunk`
surface are REPLACED here, not extended. Rewritten 2026-10-13 after
comparing Unity's Tilemap, Godot's TileMapLayer, Tiled's format and the
chunk-streaming games; the comparison first, since it decides the shape.

### What Unity, Godot and Tiled agree on

- **Nobody bakes tiles into textures.** Unity (chunk mode, 32x32x32-cell
  meshes), Godot (rendering quadrants, default 16 cells), Phaser, Unreal
  Paper2D all draw a quad per visible tile, batched per chunk. The bake is
  our deliberate divergence for the tiled-GPU primitive budget and it
  holds, but it moves memory from a few bytes per cell to texels times
  oversample squared, which is why the 260 MB happened and why residency
  with a memory readout is not optional here. The sprite layer IS the
  engines' quad path: the two layers are the two rendering modes for tile
  data (a world that interleaves sprites with tiles in depth, Godot's
  y-sort, cannot live inside a bake), and the docs say which applies when.
- **The chunk is the unit of culling, rebuild and batching, never of
  memory.** Both frustum-cull chunks and rebuild only the dirty one;
  neither evicts. Streaming is left to scenes, addons, or the app spawning
  chunks, with the Minecraft/Factorio pattern as folklore: a load radius
  wider than the view radius (hysteresis), a generate callback, a
  prioritized queue under a per-frame budget. The residency DECISION as a
  primitive is the gap both this item and
  [3d-world-residency](3d-world-residency.md) fill, with one gate.
- **Cells are unbounded.** Unity grows on write (`cellBounds`,
  `CompressBounds`), Godot has `get_used_rect`, Tiled's infinite maps are
  stored as chunks. Only Unreal Paper2D and Defold fix the size at
  creation. This supersedes the bounded contract decided in
  [2d-tile-world-bounds](../done/2d-tile-world-bounds.md): that decision
  rested on the current API surviving, which no longer binds.
- **A cell carries a transform.** Tiled stores three flip bits in every
  gid, Godot flip and transpose as alternative tiles, Unity a per-cell
  matrix and color. Without flips no Tiled map imports. The three bits
  (flipX, flipY, transpose) cover the eight symmetries of a square and are
  what the record takes; Unity's arbitrary per-cell matrix is not taken,
  a cell with its own rotation or scale is a sprite.
- **Tiles may exceed their cell.** Godot's `texture_origin`, Unity's
  oversized sprites, every isometric and hex map. Today a cell's quad is
  exactly `tileW` x `tileH`, the frame stretched to it, so an oversized
  frame would be clipped at the chunk edge. Chunk bakes need an overhang
  margin, Unity's "chunk culling bounds" in our costume.
- **Animated tiles** exist in all three (Unity AnimatedTile, Godot atlas
  animation, Tiled frame lists). In a bake an animated cell forces a chunk
  re-bake per frame change; the honest answer is a documented limit and
  the recipe (animated cells in a sprite layer), not a hidden re-bake.

### Done looks like

Seven parts that land together; the first three are one record-layout
and API change, the fourth and fifth are the shared core pieces (the gate
and the pool, both 3d-world-residency's too), the sixth is memory, the
seventh is the sprite-layer half. The split that keeps 2d and 3d
symmetric: the GATE decides, paces and pools, a HANDLER acts. The tile
layer is a built-in handler (its content is engine-owned: the bake and
the records); a 3d region's handler is the app's (its content is the
app's models); a 2d app streaming sprite populations is an app handler
too. Nothing streaming-shaped is private to tiles.ts except the bake.

1. **The data model: unbounded cells, flips, overhang.**
   `createTileLayer(tileW, tileH, atlases, opts)`: a cell is any integer
   `(col, row)`, negative included; the chunk map is sparse, keyed by chunk
   coordinates. `opts.bounds` is an optional rect clamp (writes outside
   throw) for a world that is finite by design, `usedRect()` reports the
   written extent (Unity `cellBounds`, Godot `get_used_rect`). A cell is
   frame + flips + tint + style; `setTile(col, row, frame, { flipX,
   flipY, transpose, tint, style })`, the same three bits in `setTiles`'
   index form (Tiled's gid layout: index in the low bits, flags in the
   top three, so a Tiled layer uploads unchanged). Records hold
   CHUNK-LOCAL pixel coordinates; the per-chunk `uCamera` origin places
   the chunk, so float32 precision is per chunk and a world 10^7 px from
   the origin does not shimmer (the 2d side of
   [3d-large-world-precision](3d-large-world-precision.md)). A frame
   draws at its own pixel size anchored to the cell's bottom-left (Tiled's
   rule for tiles larger than the grid; a `Frame` pivot offsets it), so
   oversized, isometric and hex tiles are expressible. Overhang is baked
   into the owning chunk: each chunk target is chunk size plus the layer's
   max overhang on every side (known from the atlases at creation), chunk
   leaves overlap by that margin, and leaves composite in row-major order
   so a lower row's overhang draws over the row above, Tiled's and Godot's
   y-order for overhanging tiles. A cell dirties exactly one chunk.
2. **Residency: the tile layer as a handler.** The resident set shrinks.
   On LEAVE a chunk's buffer and target go back to the shared pool (part
   5) and its records stay on the CPU (~5KB a chunk, ~180x cheaper than
   the texture). On ENTER a chunk takes buffer and target from the pool,
   uploads its records, bakes, and reports complete; the gate paces
   enters (part 4), the layer never decides when. `opts.retain:
   "records" | "none"`: with `"none"` records go too and `opts.fill(chunk)`
   regenerates the chunk's cells on enter (the procedural-world shape,
   Minecraft's generate), the `setTiles` over the chunk rect that costs
   microseconds, so the app is the handler there. The lifecycle is
   observable: `onEnter(chunk)` / `onLeave(chunk)` replace `chunks` +
   `onChunk`; the resident set is `resident()`. A chunk shows when its
   bake is complete, never half-baked.
3. **The camera-anchored composite, wrap-around included.** The
   component owns a clipping `<view>` sized to its box (the viewport); the
   camera transform places each resident chunk's `d-texture` leaf RELATIVE
   to the camera (the world-minus-camera subtraction in JS at leaf
   precision, so the transform's float32 stays small), rotation by
   rotating the container as in B1. Composition pruning falls out of
   residency: a leaf exists for a resident chunk, residency is "near a
   view", so the `<For>` is already the pruned set. Auto-oversample
   measures from the viewport box, not a world view (the anchor
   [2d-tile-world-bounds](../done/2d-tile-world-bounds.md) identified).
   Wrap-around is decided here, with the composite that answers it:
   `opts.wrap: { cols, rows }` makes cell coordinates modular (a torus is
   "unbounded with modular coordinates", so wrap and unbounded are one
   addressing rule, and `bounds` is the other), and the composite places
   each resident chunk at EVERY image of itself that intersects the view,
   so a view wider than the world shows the world repeated and the seam
   is two leaves of the same texture. This closes the "wrapping tile
   world" case of [2d-wrap-around](2d-wrap-around.md).
4. **The residency gate, in the spatial core, one for 2d and 3d.** The
   decision is [3d-world-residency](3d-world-residency.md)'s gate: per
   target, a spatial test in Rust, events to JS only on change, nothing
   per frame for a still camera. It has two REGION FORMS, both for both
   packages: EXPLICIT regions (a node with bounds and a load distance: a
   3d level section, a 2d platformer's room) and a GRID region set
   (origin, cell size, margin, optional bounds or wrap period), the form
   an unbounded or procedural world needs because its cells do not exist
   until approached - Unreal's World Partition and Minecraft's chunk
   columns are 2d grids streaming 3d content, so the grid form is not the
   2d form. A grid needs no BVH: the cells under a view volume are floor
   divisions. The input per target is a view volume: in 2d the AABB of
   the rotated view rect (the way Unity culls on an AABB), in 3d the
   frustum clipped at the load distance, read from the tree (the
   composite's viewport node and camera transform, a target's frustum),
   not pushed from JS. A region seen by several targets (a minimap beside
   the main view, [2d-layer-views](../done/2d-layer-views.md); a shadow
   camera or a split screen in 3d) is resident where ANY target wants
   it. Hysteresis is two radii: enter inside the smaller, leave outside
   the larger, so a camera on a chunk border never thrashes. The gate
   also PACES: enters come out prioritized by distance to the view
   centre under ONE per-frame budget shared by every consumer in the
   scene (a frame-time budget: two tile layers and a 3d region set beside
   them spend it together, never each their own), leaves come out
   immediately, and a region counts as resident when its handler reports
   complete. The handler is whoever listens: the tile layer (part 2) or
   the app.
5. **The pool, shared.** A free list over GPU buffers and render targets
   by size class, in the core beside the gate: a leaving region's
   resources go in, an entering region's come out, no create and destroy
   per approach (allocation is 1-2 ms on a desktop GPU per
   [2d-tile-bulk-writes](../done/2d-tile-bulk-writes.md)). Capped;
   beyond the cap resources are destroyed. The tile layer's handler takes
   chunk buffers and targets from it, a 3d region handler takes geometry
   from it, the 3d note's "pooling as the engine's".
6. **Memory legible, then bounded.** `get_gpu_resources` groups a layer's
   chunk textures and buffers under the layer's label with a per-layer
   total, so the number is readable before eviction exists. Then
   `opts.maxTexels`, the layer's texture budget: residency evicts
   farthest-first past it, the back pressure the monotonic allocation
   never had. Oversample stays bounded per target by `maxOversample` /
   `maxPixels` and does not shrink with content (a layer-total QUALITY
   budget would degrade as the world fills, worse than a cap); the layer
   budget bounds the resident count, not the quality.
7. **Record compaction for the live sprite layer**, the same view rects
   as input: the ordered instance buffer already gathers into draw order
   on publish, so a gather restricted to the records the view rects
   overlap (`spatial.overlap`, one core query per camera move) is the
   whole change, and `instanceCount` shrinks to the visible prefix. Only
   worth switching on past a record count where the gather is cheaper
   than the vertices it saves; measure that threshold when the first
   world of that size arrives, but build the switch here so it exists.

Bitmap-font runs ride the same machinery. The rotating-camera parity gap
for the LIVE layer is closed
([done](../done/2d-sprite-camera-rotation.md)).

### Not in this item

Each sits on the data model above and is unblocked by it, none gates
streaming:

- **Tiled import** (TMJ: layers, tilesets, flip bits, infinite-map chunks,
  animations, objects to sprites). Its own item; parts 1 and 3 are what
  it needs from the layer.
- **Terrains / auto-tiling** (Unity RuleTile, Godot terrains, Tiled Wang
  sets): a neighbour-refresh rule over `setTile`, a helper above the
  layer.
- **Isometric and hex grids**: a cell-to-world mapping; the bottom-left
  anchor and the overhang margin are what they need from the bake.
- **Animated cells**: the documented limit and the sprite-layer recipe.
- **A quad render mode for tile data**: the sprite layer is it; the docs
  say when each layer applies.

### Verification

A live example walking a world several times larger than the resident
set, read through the control API: resident count and the per-layer
total from `get_gpu_resources` flat while the camera roams; a snapshot
across a chunk boundary with an overhanging tile (seamless, lower row on
top); a wrapped world's seam snapshot (two images of one chunk); a
minimap view holding chunks resident that the main view left; a world
written 10^7 px from the origin, crisp; the pool's hit rate and the
per-frame create count under the budget from `/stats`.
