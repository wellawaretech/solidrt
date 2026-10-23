# @solidrt/2d - the baked tile layer

The baked tile layer (tiles.ts): chunks, residency, cells, views,
wrap-around worlds, bulk writes and tinting. Read ../AGENTS.md first: it
carries the model this file builds on.

## Chunks

Static 2D bulk as a few quads: on tiled GPUs the budget is primitive count
(core agents/performance.md), so a 100x100 tile world must not be 10,000
quads per frame. `createTileLayer(tileW, tileH, atlases, opts)` bakes
the world into CHUNKED `render: "manual"` targets (default ~512px of
tiles per chunk, `chunkTiles` to tune) that its views draw as one quad
each, each chunk a manual target over
the layer's ONE pipeline (its material's, resolved once per layer for its
atlases; a cell's style record is `setTile`'s `style`, its params re-bake
every resident chunk on each write) with FIXED record slots - an empty
cell is a zero-size quad, instance count is constant per chunk. Records
hold CHUNK-LOCAL pixel coordinates and every chunk target's `uCamera`
sits at minus the overhang margin, so float32 precision is per chunk and
a world 10^7 px from the origin bakes as crisp as one at the origin.
Cells are UNBOUNDED: any integer `(col, row)`, negative included, in a
sparse map of chunks keyed by chunk coordinates (Unity's growing
`cellBounds`, Godot's `get_used_rect`, Tiled's infinite maps); a world
finite by design passes `bounds` (a cell rect; writes outside throw), a
torus `wrap` (periods in cells, modular coordinates; the two exclude
each other), and `usedRect()` reports the written extent. `setTile`
batches to the frame's publish pass (the microtask, outside a frame),
whose flush publishes and re-bakes ONLY dirty resident chunks, paced
under the bake budget (Residency below). After that the layer is static
textures: zero per-frame cost however many tiles exist.

## Residency

The layer is a HANDLER over the core's residency gate (the Residency
section of @solidrt/3d's agents/large-worlds.md; flux:spatial `createGrid`): its
chunk grid is a grid of regions (`cell: [chunkW, chunkH, 0]`), every
VIEW (`layer.createView`; `<TileLayer>` makes one) is a viewer - the
view shell every layer kind shares pushes an orthographic view of its
viewport to the gate on creation and on every camera or size write,
scoped to the layer's own world (a root node the layer makes for it) -
and the gate's `spatialRegionEnter` / `spatialRegionLeave` events drive
the GPU side.
A chunk is wanted while within `enter` of any view (default half a
chunk) and kept until
beyond `leave` (default one and a half chunks; both in world pixels, the
hysteresis that keeps a camera on a chunk border from thrashing; the test
is strict, so a camera aligned to the grid never loads the ring beyond
its edge). On ENTER a chunk with cells takes a records buffer and a
target from the resource pool (`createResourcePool` in
@solidrt/core/gpu: the layer's own, 16 chunks deep, or `opts.pool`
shared with other layers and handlers; the layer registers the
destroyers of its two kinds on whichever pool it uses, so a shared pool
needs nothing from its maker), uploads its records and bakes in the same
frame's publish pass, so a chunk shows complete or not at all. The pool
hands out only what the GPU is done with: an id given back is stamped
with `submittedFrames()` and comes out again once `retiredFrames()` has
reached that count (the raster thread publishes it from its present
fences; both in @solidrt/core/gpu and `/stats`), because a frame in flight
still samples the targets and reads the buffers it was drawn with, and a
write into one of those makes a tiled driver drain its queue first (27
ms on an Adreno 610). The same rule makes a RE-BAKE of a resident chunk
(a `setTile` into it, a tint, a density change) bake into a retired set
from the pool and swap - the views rebind their entry - never into the
target on screen. And bakes are PACED, the way the gate paces enters: a
bake is a render pass the frame's picture then samples, paid beside the
frame, and a batch of them overruns a phone GPU (an Adreno 610 at a 15
ms frame pays 0.7 to 1.7 ms a bake: one a frame runs clean, two or four
miss presents as an unpaced ten-chunk batch does, 50 to 67 ms gaps), so
`setBakeBudget(n)` (default 1) is ONE
budget of bakes per frame every tile layer in the app spends together,
the residency budget one level up. A chunk's first bake, its
appearance, is never held back (the gate paced it; a hole is worse than
a late refresh) but spends the budget; the re-bakes of drawn chunks take
what is left, nearest a view's eye first across the layers (the gate's
urgency: eye distance in `leave`s), and the rest wait a frame showing
their last bake, the layer demanding the frames they need. So a
`setTint`, `setParams` or density change over forty resident chunks
rolls outward from the view over forty frames instead of landing in one
heavy frame, and an edit batch costs one frame of latency per budget of
chunks. `/stats` reports `app.tileBakes` (cumulative) and
`app.tileBakesWaiting`. The default holds on the low-end baseline; a
desktop app raises it (`Infinity` = unpaced), with a deeper `pool` past
sixteen chunks, since every re-bake parks a set until it retires. On
LEAVE the target and buffers go back to
the pool and the records stay on the CPU (~5 KB a chunk, ~180x cheaper
than its texture; `retain: "records"`, the default), or go too under
`retain: "none"`, where `opts.fill(chunk)` regenerates the chunk's cells
on the next enter with one `setTiles` over `chunk.col, chunk.row,
chunk.cols, chunk.rows` - the procedural world, Minecraft's generate
(`fill` also runs on the first approach of a never-written chunk under
`"records"`). A chunk written while not resident is records only; a
resident chunk emptied gives its target back; a leave that arrives before
a bake cancels it. `resident()` is the wanted set (a resident chunk with
no cell holds no texture), `onEnter(chunk)` / `onLeave(chunk)` bracket
it, and `TileChunk` is one object for the chunk's whole life (its cell
rect, its world rect, `texture` while resident). The gate decides WHEN
(hysteresis, the union over views, enters paced nearest first under the
scene-wide `setResidencyBudget`); the layer never does, and a layer with
no view bakes nothing. Unity and Godot cull chunks but never evict
them; the bake moves memory from bytes per cell to texels, which is why
residency is the default here, not an option.

Memory is legible, then bounded. Every chunk target and buffer carries
the LAYER's label, so `/gpu?label=<layer>` lists the layer's resources
and the reply's `groups.<layer>` is its footprint in one number
(textures, buffers, bytes: what is allocated under the label, pooled
idle targets included); `/stats` reports the gate (`regionsWanted`,
`regionsResident`, `regionsPending`, `regionsStalled`, cumulative
`regionEnters`/`regionLeaves` and the window's pair, `residentWeight`)
and the pools' traffic (`app.poolHits`, `app.poolMisses`,
`app.poolHeld`, every pool summed). `maxTexels` is the layer's texture
budget: a chunk is admitted by the grid's ESTIMATE (one target's texels
at the current density, its mip chain included, re-estimated on
`setDensity`) and weighed by its MEASURED bytes once it holds one (the
target plus its record and style buffers, `setCellWeight`; nothing
while it holds no cell), the budget is the grid's limit (the texels'
bytes plus the buffers of as many chunks as they hold, so it holds
exactly the chunks it promises), and past it the gate evicts the chunk
least needed - furthest from any view in `leave`s - for one more
needed, the eviction's leave before the enter (the evicted set reaches
the pool first; the enter takes a retired set, which that one becomes a
frame later), and a chunk nothing can make room for waits, wanted and
stalled, until a view moves or the limit changes (Unreal's streaming
pool, one grid at a time; `setResidencyLimit` is the scene-wide form
over every grid and region, bytes everywhere). The budget bounds the
resident COUNT, never the quality: a world that fills it shows holes at
its far edge rather than blur everywhere, and a resident chunk with no
cells holds no texture and weighs nothing, so a sparse world sits below
its budget. It must hold at least one chunk at the density in force
(throws otherwise, at creation and at `setDensity`). `priority` (an
integer, default 0) orders the layer among the app's grids and regions
when the enter budget is short (`setResidencyBudget`): the ground
layer before its decoration layer before the sprite populations
(Regions in views.md), and evicted last.

## Cells: frames, symmetry, overhang

A cell is a frame plus its symmetry, a tint and a style record. A frame
draws at its OWN pixel size anchored to the cell's bottom-left (Tiled's
rule for tiles larger than the grid, Unity's and Godot's own-size
sprites): a frame the cell's size fills it, a taller one reaches up, a
wider one right, so oversized, isometric and hex tiles are expressible.
The three symmetry flags `flipX`, `flipY`, `transpose` (`setTile`'s
options, or the top three bits of a `setTiles` index cell: `TILE_FLIP_X`,
`TILE_FLIP_Y`, `TILE_TRANSPOSE`, Tiled's gid layout, so a Tiled layer's
cells are these minus its firstgid) compose in Tiled's order (transpose,
then the flips) into the eight symmetries of a square with no record
layout change: the flips are UV swaps and the transpose a quarter-turn
rotation with the flips remapped (tiles-math.ts `cellSymmetry`; a
transposed frame covers `fh x fw`). `getTile` reads the canonical frame
back. Overhang is baked into the OWNING chunk: every chunk target is
chunk size plus the layer's `overhang` on every side (default the
largest overhang in the `frames` table as its frames lie, else 0; a
frame exceeding the cell plus the margin throws at the write, a
transposed non-square frame included, so give `overhang` to transpose
one), a view's chunk quads overlap by that margin and draw row-major,
so a lower row's overhang draws over the row above - Tiled's and
Godot's y-order for overhanging tiles.
A cell dirties exactly one chunk. Animated cells are a documented limit:
a bake re-renders a chunk per frame change, so animated tiles live in a
sprite layer.

## Views and the camera

The tile layer renders only through its views (`layer.createView`, the
Views section of views.md; `<TileLayer>` owns one, `<TileLayer layer>` adds
one). A tile view is a draw target with one entry per resident chunk
image over the chunk-quad pipeline (tile-pipeline.ts): the entry binds
the chunk's baked target and carries its chunk indices, the view's
shared params carry the camera, so scrolling never re-bakes and a
camera move is one params write with no entry change - the world point
(x, y) pinned to the viewport point (pivotX, pivotY), scaled by zoom,
ROTATED by rotation about the pivot, the same `CameraUpdate` the sprite
layer takes, so one signal drives a whole rotating scene across both
layers; rotation is the ship-flies-over-the-map camera. The stage takes
the camera SPLIT by chunk (`uOrigin`, the chunk the camera point lies
in, and the remainder inside it) and subtracts chunk indices before
scaling to pixels, so a world 10^7 px out renders as crisp as one at
the origin. Entries are kept in row-major world order (a lower row's
overhang over the row above) and follow residency: an entry comes when
a chunk acquires its target and goes when it releases it, synced at the
layer's flush. The view is the gate's viewer too (Residency above).
One layer has ONE bake density (`density`, texels per world pixel):
every view asks for its oversample times its camera zoom and the layer
bakes at the largest wish, rounded up with the shrink hysteresis and
bounded by `maxDensity` and `maxTexels` (a chunk must fit the budget,
so the pick is clamped, never thrown; only a pin past the budget
throws). `density` pins it, and `setDensity(null)` hands it back to the
views. A view below the density minifies the bake through the chunks'
mip chain (`mipmap`, default true; the core regenerates it after every
bake, and it counts a third more against `maxTexels`), so a minimap at
a tenth of the main zoom is as clean as the main view; `mipmap: false`
for a world no view ever minifies. The VIEW's own `oversample` (texels
per view pixel, the auto pick from the leaf's on-screen box in the
component) is the sprite view's knob, unchanged. Tiles are data, not
children: there is no `<Tile>` component on purpose - write cells
through `ref` with `setTile`; a `<Camera2d>` child drives the view.

## Wrap-around worlds

A toroidal world - the Asteroids screen wrap, a world map that scrolls
east forever - is three cases, two of them app recipes (Unity, Godot
and Three leave entity wrap to the game too):

- **A repeating background** is a quad sampling its texture with `wrap:
  "repeat"` (SamplerOptions on the gpu face, or a per-binding `{ id,
  wrap }` override) and a camera-driven UV offset: the GPU wraps, no
  seam exists. The `<texture>` element form is
  okf/backlog/texture-tile-mode.md.
- **Entity wrap** is position modulo per step, `((x % W) + W) % W`, on a
  JS-moved sprite (records layer or `setSprite` per frame). The TRAP: a
  sprite with a declared position `transition` treats the wrap write as
  a target and animates the jump clear across the world - there is no
  teleport write on a transitioned sprite, so a wrapping entity is a
  JS-motion workload. Seam continuity is GHOST COPIES: a sprite
  straddling an edge is drawn twice (four times in a corner), so park
  permanent ghosts off-camera and place them when the primary is within
  half a sprite of an edge; an in-shader modulo could never replace this
  (a quad crossing the seam needs a second draw, not a moved one).
- **A wrapping tile world** is `wrap: { cols, rows }` on the tile layer
  (periods in cells, whole chunks each; `bounds` and `wrap` exclude each
  other): cells are modular, and a view draws every IMAGE of a resident
  chunk inside its world box, so the seam is two entries over one
  texture and a view wider than the world shows it repeated, in exact
  painter's order. The image set depends on the camera only through the
  chunk-index range the view's box covers - four integers compared on
  every camera write - so a scroll costs no JS until the view crosses an
  image boundary.

## Bulk writes

Bulk writes: `setTiles(col, row, cols, rows, cells)` writes a rect at
once - `cells` row-major, `cols * rows` long, either frames (null clears)
or, with a `frames` table given at creation (`createTileLayer(..., {
frames })`, the `<TileLayer frames>` prop: a tileset, `grid()`'s array),
indices into it with the symmetry flags in the top three bits (-1
clears; a Uint16Array holds -1 as 0xffff and a Uint32Array as
0xffffffff, the same bits) - Unity's SetTilesBlock, Phaser's putTilesAt
over the map's tileset. One locate and one dirty mark per chunk the rect
touches, the per-cell loop inside the layer, the same flush; a chunk the
rect only clears never allocates, so a whole-world write of a sparse
world allocates exactly the chunks the per-cell seed would. A generated
or worker-built world is a Uint16Array or Uint32Array of indices,
transferable, never an array of frame objects; a fill is `new
Uint16Array(n).fill(i)`. `tint`, `style` and the frame-form flags in the
options apply to every cell set. Every entry is validated before the
first write, so a bad one changes nothing; the table is copied at
creation. Under `wrap` a rect may cross the seam but not exceed a period.
The rect form is the shape for worker transfer and chunk fill, not a
large speedup: the per-cell floor is the record write itself.

## Tinting

Tinting, two levels: `setTile(col, row, frame, { tint })` writes the
cell's record tint (same `[r, g, b, a]` 0..1 semantics as a sprite's
tint; absent keys keep their values, a cell set from empty starts at
`[1, 1, 1, 1]`), and `setTint(rgba)` / the `tint` option and prop tints
the whole layer through the shared `uTint` uniform, multiplied over the
per-cell tints - day/night, a dimmed parallax plane. A layer-tint write
is not a record write (chunks re-render GPU-side, nothing re-uploads),
but it does re-render every resident chunk: drive it from slow state,
not per frame. `setParams` (a material prelude's uniforms) and a cell's
`style` (the material's per-cell record, `setTile`/`setTiles` `{ style
}`) are the same two levels for a custom look, see materials.md. Not
built: record compaction for the live sprite layer (a gather restricted
to the records the views see; okf/backlog/2d-sprite-record-compaction.md
holds the design and the condition it waits on).

## Traps

- `<TileLayer>` is a `<SpriteLayer>` in shape: a `<texture>` leaf
  showing its view, filling its parent when unsized (give it a sized
  parent) or `width`/`height` in view pixels. Both layers render
  LAID-OUT leaves and take no position props, so to overlay sprites on
  tiles wrap each in a `<view position="absolute">` inside one container
  - as plain flex siblings they stack side by side instead. Neither can
  live inside a d-* subtree (the insert throws; `output` with a
  `<d-texture>` is the detached form).
- A layer with NO VIEW bakes nothing: imperative use (no `<TileLayer>`)
  makes one with `layer.createView({ width, height, camera })` and shows
  `view.texture`, or every chunk stays records on the CPU. `resident()`
  is empty, and a chunk's `texture` null, until the gate's enter arrives
  with a frame - a test settles or steps a frame before reading a
  chunk's target or the view's.
- `setCamera` is a PARTIAL update on every view (absent keys keep their
  values, the params rule): `view.setCamera({})` moves nothing; a reset
  is `{ x: 0, y: 0 }`.
- A frame bakes at its OWN size, anchored to the cell's bottom-left: a
  one-texel atlas no longer fills an 8 px cell (the sprite layers stretch
  a frame to the sprite's size; the tile layer does not). A frame larger
  than the cell plus `overhang` throws at the write; the `frames` table's
  largest frame sets the default margin, so a Tiled tileset with
  oversized tiles works unconfigured.
- `chunkClearColor` is PER CHUNK, as named: regions with no cell have no
  texture and render nothing, so a full-bleed ground color is the view's
  `clearColor` (or the container behind the leaf), not this. The flip
  side is FREE TRANSPARENCY: with the default `[0,0,0,0]` clear, a
  mostly-empty tile layer stacked over another composites with no mask,
  no alpha pass, no shader - a sparse upper world (floating clouds over
  a sea) just works, and per-cell alpha in the written cells carries
  through the chunk (transparent clear + alpha blend).
- Two densities, two words: the LAYER's `density` is texels per WORLD
  pixel (the bake), a VIEW's `oversample` is texels per VIEW pixel (the
  target); the view's wish for the bake is their product with the zoom.
  Hard pixels belong to the ATLAS sampler (`createAtlas` `filter:
  "nearest"`): the view samples the chunks linear through their mip
  chain, which at a 1:1 density-to-view mapping is exact and everywhere
  else the proper resample.
- A dirty chunk flush publishes and re-bakes that chunk in full, into a
  retired target and buffers from the pool (the ones on screen stay with
  the frames in flight; writing them stalls a phone GPU for tens of
  milliseconds), at most the bake budget of chunks a frame
  (`setBakeBudget`, default 1, nearest a view first; the rest show their
  last bake a frame longer). Fine on change-only cadence; per-frame
  setTile churn over more chunks than the budget falls behind by design,
  and each re-bake parks a set until it retires - that is sprite-layer
  work, not tile work. An animated tile is a sprite.
- Memory is the RESIDENT set times the chunk texture (~1.3 MB at the
  default size and density 1 with its mip chain, more with the overhang
  margin and the density squared), not the touched area: a leave gives
  the texture to the pool, the pool's cap bounds what waits there, and
  `maxTexels` bounds the resident set (`groups.<label>` in `/gpu` is the
  footprint).
- Under `wrap` the periods must be whole chunks: the default `chunkTiles`
  is picked to divide them, an explicit one that does not throws. A rect
  write wider than a period throws too.
