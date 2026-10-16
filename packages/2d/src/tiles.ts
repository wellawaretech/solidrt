// The baked tile layer: an unbounded grid of atlas frames rendered into
// CHUNKED `render: "manual"` targets and shown through VIEWS as a handful
// of quads - never a quad per tile. On tiled GPUs the budget is primitive
// count, so a 100x100 world must not be 10,000 quads per frame; baked, it
// is a few chunk textures, and scrolling is a camera write on a view,
// never a repaint.
//
// Cells are unbounded (any integer, negative included; a `wrap` period
// makes them modular, `bounds` clamps them) and live in a sparse map of
// chunks keyed by chunk coordinates. Each chunk is a small layer of its
// own: fixed record slots (record localRow * chunkTiles + localCol IS that
// cell, an empty cell a zero-size quad, instance count constant), records
// in CHUNK-LOCAL pixel coordinates so float32 precision is per chunk and a
// world 10^7 px from the origin does not shimmer, the chunk target's
// `uCamera` at minus the overhang margin. A cell is a frame plus its
// symmetry (flipX, flipY, transpose: UV swaps and a quarter-turn rotation,
// tiles-math.ts), a tint and the material's style record; a frame draws at
// its OWN pixel size anchored to the cell's bottom-left, so oversized,
// isometric and hex tiles are expressible, and the overhang bakes into the
// owning chunk (its target is chunk size plus the layer's max overhang on
// every side, the views overlap the quads by that margin).
//
// Views: the layer renders only through its views (createView, the
// sprite layer's contract one layer kind over; views.ts holds the shell).
// A view is a draw target holding one entry per resident chunk image over
// the chunk-quad pipeline (tile-pipeline.ts): the entry binds the chunk's
// baked target and carries its chunk indices, the view's shared params
// carry the camera, so a camera move is one params write and no entry
// changes. Entries are kept in row-major world order (a lower row's
// overhang draws over the row above, Tiled's and Godot's y-order), and the
// set follows residency: an entry comes when a chunk acquires its target
// and goes when it releases it. Under `wrap` a chunk has one entry per
// IMAGE of itself inside the view's world box, so the seam is two entries
// over one texture; the image set depends on the camera only through
// four integers (the chunk-index range the view's box covers), compared
// on every camera write and rebuilt on a change - a scroll costs no JS
// until the view crosses an image boundary. Chunk targets carry a mip
// chain by default, so a view below the bake density (a minimap at a
// tenth of the main zoom) minifies without aliasing.
//
// Residency: the layer is a HANDLER over the core's residency gate
// (flux:spatial createGrid, the Residency section of @solidrt/3d's
// AGENTS.md): its chunk grid is a grid of regions, every view is a viewer
// wanting the chunks near its viewport (its camera is pushed to the gate
// as an orthographic view), and the gate's enter and leave events drive
// the GPU side. On ENTER a chunk takes a records buffer and a target from
// the shared pool (createResourcePool in @solidrt/core/gpu), uploads its
// records and bakes in the same frame's publish pass; on LEAVE they go
// back to the pool and the records stay on the CPU (~5 KB a chunk) - or
// go too under `retain: "none"`, where `fill` regenerates the chunk on
// the next enter. The gate decides when (hysteresis, the union over
// views, the paced budget); the layer never does. A layer with no view
// bakes nothing.
//
// setTile and setTiles batch; the flush (ahead of the frame's paint, or at
// a microtask outside a frame) publishes and re-bakes ONLY dirty resident
// chunks and syncs the views' entries, and it is ONE flush for every tile
// layer in the app: the re-bakes of drawn chunks are paced under the bake
// budget (setBakeBudget, the gate's residency budget one level up: bakes
// per frame, nearest a view's eye first, the rest wait a frame showing
// their last bake), because a tiled GPU pays each bake beside the frame
// and a batch of them past a handful overruns it. A layer nobody edits
// and nobody moves over costs nothing per frame.
import { displayScale, getOwner, onBeforeRender, onCleanup, runWithOwner, untrack, windowSize } from "@solidrt/core"
import {
  addDraw,
  beginBufferWrite,
  createBuffer,
  createDrawTarget,
  createResourcePool,
  createShaderTarget,
  destroyBuffer,
  destroyTexture,
  endBufferWrite,
  isFloatLayout,
  layoutFields,
  layoutStride,
  limits,
  removeDraw,
  renderTarget,
  setDraw,
  setDrawOrder,
  setDrawTextures,
  setTargetParams,
  setTargetSize,
  writeBuffer,
  writeRecord,
} from "@solidrt/core/gpu"
import type { AttributeAccess, BufferId, DrawId, ResourcePool, ShaderParams, TextureId, VertexAttribute } from "@solidrt/core/gpu"
import { requestFrame } from "flux:rendertree"
import * as spatial from "flux:spatial"
import type { GridId, ViewKey } from "flux:spatial"
import { registerStat } from "sol:dev"
import { on } from "sol:events"
import { checkAtlases, frameIndex } from "./atlas.ts"
import type { Atlas } from "./atlas.ts"
import { unprojectCamera } from "./camera.ts"
import type { CameraState } from "./camera.ts"
import { isFrame, writeFrame } from "./frames.ts"
import type { Frame } from "./frames.ts"
import { atlasBindings, RECORD_ATTRIBUTES } from "./glsl.ts"
import { checkParams, checkTint } from "./layer.ts"
import { materialBlank, materialSamplers, unlit } from "./material.ts"
import type { Material } from "./material.ts"
import { pickOversample } from "./oversample-math.ts"
import { checkOversample, thrashSentinel } from "./oversample.ts"
import { INSTANCE_FLOATS } from "./records.ts"
import { CHUNK_SAMPLER, tileCameraParams, tilePipeline } from "./tile-pipeline.ts"
import { cellSymmetry, checkCell, checkRect, checkTileRect, chunkKey, eachChunkSlice, flagsOf, floorDiv, footprint, indexOf, isClearIndex, mod, slotOf } from "./tiles-math.ts"
import type { TileRect } from "./tiles-math.ts"
import { createViewShell } from "./views.ts"
import type { ViewHandle, ViewOptions } from "./views.ts"

const RESOLVED = Promise.resolve()

// Default chunk edge in pixels; the tile count per chunk derives from it.
const CHUNK_TARGET_PX = 512
// Default residency margins in chunk edges: a chunk enters half a chunk
// before its edge reaches the view and leaves one chunk past that, the
// band that keeps a camera on a chunk border from thrashing.
const DEFAULT_ENTER_CHUNKS = 0.5
const DEFAULT_LEAVE_CHUNKS = 1.5
// The default pool's capacity in chunks' worth of resources (a target and
// the chunk's buffers each): what a camera sweeping back and forth reuses
// before a chunk is created again.
const POOL_CHUNKS = 16
// What a mip chain adds to a target's level-0 texels (the geometric sum
// of the quarter-size levels): what a chunk weighs against `maxTexels`
// with its chain on.
const MIP_CHAIN_FACTOR = 4 / 3

export type Tint = [number, number, number, number]

// Floats per frames-table entry: the four UVs, the atlas index, and the
// frame's pixel size.
const TABLE_FLOATS = 7
// Float offsets inside a record (records.ts layout).
const ROT_FIELD_OFFSET = 8
const ATLAS_FIELD_OFFSET = 15

export type { TileRect } from "./tiles-math.ts"
export { TILE_FLIP_X, TILE_FLIP_Y, TILE_TRANSPOSE } from "./tiles-math.ts"

export type TileLayerOptions = {
  /** Per-chunk clear color (the name says the scope: chunks that hold no
   * cell render nothing, so this is not a layer background). */
  chunkClearColor?: [number, number, number, number]
  /**
   * What the cells look like: the layer's material (see
   * SpriteLayerOptions.material), fixed at creation; default `unlit()`.
   * Its blend is how the cells bake into their chunks ("add" for a glow
   * or light map baked over the transparent chunk clear), its params and
   * textures reach every chunk target, its instance buffers are a style
   * record per cell (`setTile`'s `style`). A tile layer BAKES, so a
   * param change re-bakes every resident chunk, like the tint (under the
   * bake budget, see setBakeBudget): animated uniforms belong on a
   * sprite layer.
   */
  material?: Material
  /** Uniform values seeded on every chunk over the material's own; see
   * TileLayer.setParams. */
  params?: ShaderParams
  /**
   * Pin the bake density: target texels per world pixel in the chunks
   * (positive integer). Absent (the default), the layer picks it from its
   * VIEWS: the largest target texels per world pixel any view asks for
   * (its oversample times its camera zoom), rounded up, with a shrink
   * hysteresis, bounded by `maxDensity`, by the window's own pixel count
   * per chunk, and by `maxTexels` (a chunk must fit the budget). One
   * density serves every view of the layer; a view below it minifies the
   * bake through the chunks' mip chain (`mipmap`). See
   * TileLayer.setDensity.
   */
  density?: number
  /**
   * Cap on the auto-picked density (integer >= 1): chunks stay adaptive
   * up to it, never above. A tile world's texture memory is resident
   * chunks x n squared, so this is the lever that bounds it on
   * high-scale displays without giving up adaptivity; a chunk is
   * world-sized, so the cap is one cost on every device. Ignored under a
   * pinned `density`.
   */
  maxDensity?: number
  /**
   * Whether the chunk targets carry a mip chain (default true): the core
   * regenerates it after every bake, so a view below the bake density -
   * a minimap at a tenth of the main view's zoom - minifies the bake
   * without aliasing. A chain adds a third to a chunk's texels, counted
   * against `maxTexels`; a world no view ever minifies may drop it.
   */
  mipmap?: boolean
  /**
   * Chunk edge in TILES; default sized so a chunk is ~512px (under `wrap`,
   * the largest divisor of both periods not above that). The tuning knob
   * between re-bake granularity (smaller = finer dirty regions) and chunk
   * count (larger = fewer textures and leaves). Under `wrap` it must
   * divide both periods, so the seam falls on a chunk edge.
   */
  chunkTiles?: number
  /** Layer tint, [r, g, b, a] in 0..1; see TileLayer.setTint. */
  tint?: [number, number, number, number]
  /**
   * The frames table setTiles' index cells name - a tileset: `grid()`'s
   * array, or any Frame[] (up to 2^29 entries, from any of the layer's
   * atlases), so a generated world is a Uint16Array or Uint32Array of
   * indices and never an array of frame objects. Fixed at creation; setTile
   * takes frames directly either way. Its largest frame sets the layer's
   * overhang margin (see `overhang`).
   */
  frames?: Frame[]
  /**
   * A world finite by design: the rect of cells that exist (Unity's
   * `cellBounds` as a clamp). Writes outside throw; the residency grid
   * never enumerates chunks outside. Without it (and without `wrap`) any
   * integer cell exists, negative included. Not with `wrap`.
   */
  bounds?: TileRect
  /**
   * A torus: cell coordinates are modular in these periods (cells), so
   * `setTile(cols + 1, row)` writes cell 1 and a view repeats the world
   * where it crosses the seam (every image of a resident chunk inside
   * the view is drawn). Both periods must be whole chunks
   * (`chunkTiles` divides them; the default is picked to). Not with
   * `bounds`.
   */
  wrap?: { cols: number; rows: number }
  /**
   * World pixels a frame may extend past its cell (a frame draws at its
   * own pixel size, anchored to the cell's bottom-left, so an oversized
   * tile reaches right and up): every chunk target is chunk size plus
   * this margin on every side and the views overlap the quads by it.
   * Default: the largest overhang in the `frames` table as its frames
   * lie (a transposed non-square frame covers `fh x fw`, so give the
   * margin explicitly to transpose one), else 0. A frame exceeding the
   * cell plus this margin throws at the write.
   */
  overhang?: number
  /**
   * The residency band in world pixels (RegionMargins): a chunk enters
   * while within `enter` of a viewer's view and stays until beyond
   * `leave` (`leave >= enter`). Defaults: half a chunk and one and a half
   * chunks.
   */
  enter?: number
  leave?: number
  /**
   * The layer's texture budget in texels: the most its resident chunks
   * may hold at once, counted as chunk targets at the current density,
   * mip chain included (a resident chunk with no cells holds no texture
   * but counts as one).
   * Past it the residency gate evicts the chunk least needed, the
   * furthest from any viewer in `leave`s, for one more needed; a chunk
   * nothing can make room for waits, wanted, until a view moves
   * (flux:spatial setResidencyLimit). Must hold at least one chunk
   * (throws otherwise); the default is no budget. A budget bounds the
   * resident COUNT, not the quality: the density stays what the views
   * ask (`maxDensity` bounds that), so a world that fills the budget
   * shows holes at its far edge rather than blur everywhere.
   */
  maxTexels?: number
  /**
   * What a chunk keeps when it leaves: `"records"` (default) keeps its
   * cells on the CPU (~5 KB a chunk, ~180x cheaper than its texture) so
   * the next enter re-uploads and bakes; `"none"` drops them too, and
   * `fill` regenerates the chunk on enter - the procedural-world shape.
   */
  retain?: "records" | "none"
  /**
   * Called on enter for a chunk that holds no cells yet - every enter
   * under `retain: "none"`, the first approach under `"records"` - to
   * generate its cells: a `setTiles` over `chunk.col, chunk.row,
   * chunk.cols, chunk.rows` costs microseconds. Runs before `onEnter`.
   */
  fill?: (chunk: TileChunk) => void
  /**
   * The resource pool chunk buffers and targets are taken from on enter
   * and given back to on leave (createResourcePool): the layer's own by
   * default, sized for 16 chunks; share one across layers and other
   * residency handlers by passing it - the layer registers the
   * destroyers of its own kinds on it (destroyWith), so a shared pool
   * needs nothing from its maker.
   */
  pool?: ResourcePool
  label?: string
  /** Skip the owner-scoped auto-dispose (see createTileLayer). */
  autoFree?: boolean
}

/**
 * One chunk of the world: its cell rect, its world-pixel rect (without
 * the overhang margin) and, while resident, its baked target. The same
 * object for the chunk's whole life, so a handler may key on it.
 */
export type TileChunk = {
  /** The chunk's first cell and its size in cells (chunkTiles square). */
  readonly col: number
  readonly row: number
  readonly cols: number
  readonly rows: number
  /** World-pixel origin and size of the chunk's rect (its cells, not the
   * overhang margin). */
  readonly x: number
  readonly y: number
  readonly width: number
  readonly height: number
  /** The chunk's target while it is resident and holds a cell, else null:
   * baked at the flush of the frame its enter lands in, and after an edit
   * showing its last bake until the re-bake's turn under the bake budget
   * (setBakeBudget). The texture is `width + 2 * overhang` wide (times
   * the density) with its top-left at `x - overhang, y - overhang`. */
  readonly texture: TextureId | null
}

/** A cell write's options: the cell's symmetry, its tint and, under a
 * material with instance buffers, its style record. */
export type TileCellOptions = {
  /** Mirror the frame's columns (default false). */
  flipX?: boolean
  /** Mirror the frame's rows (default false). */
  flipY?: boolean
  /** Reflect the frame across its diagonal, applied before the flips as
   * in Tiled (default false): the three together are the eight
   * symmetries of a square. A transposed frame covers `fh x fw`. */
  transpose?: boolean
  /** The cell's tint, multiplied over its texels like a sprite's; absent
   * keeps the cell's current tint. */
  tint?: Tint
  /** The cell's style record (the material's FIRST instance buffer, one
   * value per component in attribute order), absent keeps it; a cell set
   * from empty starts at the material's instanceStyle. Throws on a
   * material without an instance buffer. */
  style?: ArrayLike<number>
}

export type TileLayer = {
  /** The atlases the layer bakes from, as declared at creation: every
   * frame set names one of their textures (see LayerBase.atlases). */
  readonly atlases: readonly Atlas[]
  /** The material the layer bakes with, as declared at creation (the
   * stock `unlit()` when none was). */
  readonly material: Material
  /** Cell size in world pixels, fixed at creation. */
  readonly tileW: number
  readonly tileH: number
  /** Chunk edge in cells. */
  readonly chunkTiles: number
  /** Chunk size in world pixels (chunkTiles * tile size): a chunk's rect,
   * before the overhang margin and the density. */
  readonly chunkW: number
  readonly chunkH: number
  /** The overhang margin in world pixels (see TileLayerOptions.overhang). */
  readonly overhang: number
  /** The cell bounds and the wrap periods, as given at creation. */
  readonly bounds: TileRect | null
  readonly wrap: { cols: number; rows: number } | null
  /** Target texels per world pixel, the bake density in force (pinned, or
   * the views' pick); see setDensity. */
  readonly density: number
  /**
   * Pin the bake density at `n` target texels per world pixel (positive
   * integer): every resident chunk re-bakes once at the new size, under
   * the bake budget nearest a view first (the picture sharpens outward
   * over a few frames); world pixels, records and the cameras are
   * untouched, and the views' wishes stop applying. Throws past
   * `maxTexels` (a chunk must fit the budget). `null` returns the density
   * to the views' pick (TileLayerOptions.density).
   */
  setDensity(n: number | null): void
  /** Whether the chunk targets carry a mip chain, as declared at
   * creation (TileLayerOptions.mipmap). */
  readonly mipmap: boolean
  /**
   * A view of the world: the sprite layer's createView one layer kind
   * over - a draw target with a camera and a viewport of its own, shown
   * as `view.texture` (`<texture src>`; `<TileLayer>` is the component
   * form) and the viewer the residency gate keeps chunks resident around:
   * the view's camera over its viewport is pushed to the gate on every
   * camera, size or oversample write, and `oversample * zoom` is its wish
   * for the bake density (the layer bakes at the largest wish). A layer
   * with no view bakes nothing. The view draws one quad per resident
   * chunk image, sampling the bake, so a camera move costs one params
   * write; its `pick` is empty (a tile view hits no sprite) and its
   * listeners see every pointer event over the world in world pixels.
   * Disposed with the layer.
   */
  createView(opts: ViewOptions): ViewHandle
  /** The resident chunks (entered, not left), in no particular order: a
   * fresh array. A resident chunk holding no cell has no texture. */
  resident(): TileChunk[]
  /** Called after a chunk enters (after `fill`, its resources taken and
   * its bake queued for this frame) and after it leaves (its resources
   * back in the pool). Assignable. */
  onEnter?: (chunk: TileChunk) => void
  onLeave?: (chunk: TileChunk) => void
  /**
   * Set one cell: a frame draws it, null clears it (a clear ignores `opts` -
   * an absent cell has no tint, it draws nothing). The frame draws at its
   * own pixel size anchored to the cell's bottom-left, under the cell's
   * symmetry flags (default none), tinted by `tint` (absent keeps the
   * cell's current tint; a cell set from empty starts at [1, 1, 1, 1]).
   * Batched; the flush ahead of the frame's paint (the microtask, outside
   * a frame) publishes and re-bakes ONLY the resident chunks that changed,
   * however many tiles did - at most the bake budget of them per frame
   * (setBakeBudget), nearest a view first, the rest showing their last
   * bake until their turn. A write to a non-resident chunk is records
   * only: it bakes when the chunk enters.
   */
  setTile(col: number, row: number, frame: Frame | null, opts?: TileCellOptions): void
  /**
   * Set a rect of cells at once: `cols` x `rows` cells from `col`, `row`,
   * `cells` in row-major order (`cols * rows` long) - frames (null clears)
   * or, with a `frames` table on the layer, indices into it with the
   * symmetry flags in the top three bits (TILE_FLIP_X, TILE_FLIP_Y,
   * TILE_TRANSPOSE: Tiled's gid layout, so a Tiled layer's cells are
   * these minus its firstgid; -1 clears, which a Uint16Array holds as
   * 0xffff and a Uint32Array as 0xffffffff). Unity's SetTilesBlock,
   * Phaser's putTilesAt: one locate and one dirty mark per chunk the rect
   * touches, the per-cell loop inside the layer over its own records, the
   * same batching and flush. `tint` applies to every cell set, as
   * setTile's, and so do `style` and the frame-form flags; absent, each
   * cell keeps its tint and style. A bad entry throws before any cell
   * changes. Under `wrap` the rect may cross the seam but not exceed a
   * period.
   */
  setTiles(col: number, row: number, cols: number, rows: number, cells: ArrayLike<Frame | null> | ArrayLike<number>, opts?: TileCellOptions): void
  /** The frame at a cell (in its canonical orientation), or null when
   * empty or unknown to the layer (a chunk dropped under retain "none"). */
  getTile(col: number, row: number): Frame | null
  /** The rect of cells holding a frame, or null when none does (Unity's
   * cellBounds, Godot's get_used_rect): what the layer knows, so under
   * retain "none" the resident chunks. */
  usedRect(): TileRect | null
  /** The frames table given at creation (what setTiles' indices name), or
   * null. */
  readonly frames: readonly Frame[] | null
  /**
   * Tint the whole layer, [r, g, b, a] in 0..1: a uniform multiplied over
   * every cell's own tint (day/night, a dimmed parallax plane). Not a
   * record write - resident chunks re-bake GPU-side with nothing
   * re-uploaded - but each write does re-bake every resident chunk
   * (under the bake budget, so a layer of many chunks takes a few frames
   * to roll over, nearest a view first), so animate it coarsely, not per
   * frame.
   */
  setTint(tint: [number, number, number, number]): void
  /**
   * Write uniform values to every resident chunk (the material's prelude
   * uniforms, over its own `params`): LayerBase.setParams for a baked
   * layer, so each write re-bakes every resident chunk under the bake
   * budget, like setTint - drive it from slow state, not per frame.
   * Absent names keep their values; a name the program does not declare
   * is skipped.
   */
  setParams(params: ShaderParams): void
  dispose(): void
}

type Chunk = TileChunk & {
  texture: TextureId | null
  key: number
  chunkCol: number
  chunkRow: number
  /** The cells, null until the first write (and after a leave under
   * retain "none", when the chunk object itself goes). */
  records: Float32Array | null
  /** The records' GPU buffer while resident. */
  buffer: BufferId | null
  /** The material's style records of the chunk, one array (and buffer
   * while resident) per instance buffer, uploaded whole with the records. */
  styles: ChunkStyle[]
  /** Cells holding a frame. */
  count: number
  /** The gate wants it (entered, not left). */
  wanted: boolean
  dirty: boolean
  /** Records changed since the last flush (a tint-only or resize re-bake
   * renders without re-uploading them). */
  wrote: boolean
  /** The target and buffers have never been drawn by a view: the pending
   * bake may write them in place. Once drawn, a re-bake takes a retired
   * set from the pool and swaps, since the frames in flight still read
   * the old one. */
  fresh: boolean
}

type ChunkStyle = { bytes: Uint8Array; buffer: BufferId | null; fields: AttributeAccess[]; floats: Float32Array | null }

// A view's draw entry over one chunk image: the entry and the chunk
// target it binds (a chunk back from the pool may hold another id).
type ImageEntry = { draw: DrawId; texture: TextureId }

// One view of the layer: the shell, its gate key, its entries by image
// key, and under wrap the chunk-index range its world box covers
// ([c0, c1, r0, r1], the image set's only camera dependence).
type TileView = {
  view: ViewHandle
  key: ViewKey | null
  entries: Map<number, ImageEntry>
  range: [number, number, number, number] | null
  /** The entry set may have changed (residency, a write, a range
   * change): sync at the flush. */
  dirty: boolean
}

// An image wanted by a view: its key and its chunk indices (the
// canonical chunk's offset by whole periods under wrap).
type Image = { key: number; col: number; row: number; chunk: Chunk }

// One style stream of the material as the tile layer keeps it: the
// layout, the blank record, and the stride.
type StyleShape = { layout: VertexAttribute[]; blank: Uint8Array; stride: number }

// The pool kinds the layer gives and takes under: a target is
// interchangeable with any over the same pipeline (setTargetSize resizes
// it id-stable), a buffer with any of its byte size. The size each pooled
// target holds is kept by id across every layer (ids are unique), so a
// take at the same size skips the resize and the target's destroyer,
// registered on whichever pool a layer uses, forgets it.
const TARGET_KIND = "tile-target"
const BUFFER_KIND = "tile-buffer"
let targetSizes = new Map<TextureId, number>()
function freeTarget(id: number): void {
  targetSizes.delete(id as TextureId)
  destroyTexture(id as TextureId)
}
function freeBuffer(id: number): void {
  destroyBuffer(id as BufferId)
}

// The gate's events are routed to the layer owning the grid: one lazy
// subscription per event, keyed by grid id. The frame's start is
// subscribed beside them, for the bake budget's accounting.
let handlers = new Map<GridId, { enter(i: number, j: number): void; leave(i: number, j: number): void }>()
let subscribed = false

function subscribe(): void {
  if (subscribed) return
  subscribed = true
  on("frameStart", () => {
    bakesSpentInFrame = bakesSpentForNext
    bakesSpentForNext = 0
  })
  on("spatialRegionEnter", (event: { grid?: GridId; cell?: [number, number, number] }) => {
    if (event.grid === undefined || event.cell === undefined) return
    handlers.get(event.grid)?.enter(event.cell[0], event.cell[1])
  })
  on("spatialRegionLeave", (event: { grid?: GridId; cell?: [number, number, number] }) => {
    if (event.grid === undefined || event.cell === undefined) return
    handlers.get(event.grid)?.leave(event.cell[0], event.cell[1])
  })
}

// ---- The bake budget ---------------------------------------------------
//
// A bake is a render pass into a chunk target that the frame's picture
// then samples, and a tiled GPU pays it beside the frame: on an Adreno
// 610 at a 15 ms base frame a bake costs 0.7 to 1.7 ms, one a frame
// holds 60 fps with no missed present, and two or four a frame miss
// presents as an unpaced ten-chunk batch does (50 to 67 ms gaps;
// okf/backlog/2d-tile-world-frame-hitches.md). So bakes are PACED, the
// way the gate paces enters: one budget of bakes per frame for every
// tile layer in the app (the gate's residency budget one level up; two
// layers edited together spend it, never each their own). A chunk's
// first bake, its appearance, is never held back - the gate's enter
// budget paced it, and a hole is worse than a late refresh - but it
// spends the budget; the re-bakes of drawn chunks take what is left,
// nearest a view's eye first across the layers, and the rest wait a
// frame showing their last bake.

// Bakes per frame when no app set a budget: one, what the low-end baseline
// (an Adreno 610 at a near-budget frame) holds clean, measured 2026-10-16
// on the tiles example; an app on desktop-class GPUs raises it.
const DEFAULT_BAKE_BUDGET = 1
let bakeBudget = DEFAULT_BAKE_BUDGET
// The budget is spent per frame of GPU work. The bakes a frame's publish
// pass issues join that frame's work; the bakes a flush between frames
// issues (a write outside a frame, at its microtask) join the NEXT
// frame's. So the spend has two slots, and the next frame's start carries
// the second over into the first. Every frame that runs resets the spend,
// whether or not it drew: the bakes themselves demand the draw, and a
// frame whose publish pass found no budget left would otherwise never
// see a new one.
let bakesSpentInFrame = 0
let bakesSpentForNext = 0
// Which slot the flush under way charges (set by flushLayers).
let flushInFrame = false
// Every layer's bakes so far, and the re-bakes waiting on the budget as
// of the last flush: the stats (`app.tileBakes`, `app.tileBakesWaiting`).
let bakesTotal = 0
let bakesWaiting = 0
registerStat("tileBakes", () => bakesTotal)
registerStat("tileBakesWaiting", () => bakesWaiting)

/**
 * The chunk bakes per frame every tile layer in the app spends together
 * (default 1): a chunk's first bake always runs and counts, the re-bakes
 * of drawn chunks (a `setTile` into one, `setTint`, `setParams`,
 * `setDensity`) take what is left, nearest a view's eye first, and the
 * rest wait a frame showing their last bake. The lever against the frame
 * hitch a batch of bakes causes on a tiled GPU: the default holds on the
 * low-end baseline, a desktop app may raise it, and `Infinity` runs every
 * bake in the frame it was asked in. A layer's default pool holds sixteen
 * chunks' worth, so a budget past that churns the pool on a whole-layer
 * re-bake: pass a deeper `pool`. A positive integer or Infinity; throws
 * otherwise.
 */
export function setBakeBudget(bakes: number): void {
  if (!((Number.isInteger(bakes) && bakes >= 1) || bakes === Infinity)) {
    throw new Error(`setBakeBudget: bakes per frame must be a positive integer or Infinity, got ${bakes}`)
  }
  bakeBudget = bakes
}

function bakesSpent(): number {
  return flushInFrame ? bakesSpentInFrame : bakesSpentForNext
}
function spendBake(): void {
  if (flushInFrame) bakesSpentInFrame++
  else bakesSpentForNext++
  bakesTotal++
}

// One flush for every tile layer: a layer registers its part here, a
// write outside a frame runs it at the microtask (between frames: the
// checkpoint comes after a frame's whole closure), and the publish pass of
// every frame runs it ahead of the paint while anything is pending, so a
// tile set anywhere in the frame's JS is baked for that frame's picture.
// The hook is installed once, outside any owner (a layer may outlive the
// owner it was created in), and is a flag check when nothing is pending.
type LayerFlush = {
  /** Whether a view of the layer moved since the last flush (the spatial
   * core is flushed once for every layer then). */
  takeViewsDirty(): boolean
  /** Bake the layer's fresh dirty chunks now and list its drawn dirty
   * chunks as candidates for the budget. */
  prepare(out: Candidate[]): void
  bake(chunk: Chunk): void
  /** Bring the views whose entry set changed up to date. */
  sync(): void
}
// A drawn chunk waiting to re-bake: its urgency (the layer's measure,
// comparable across layers), its key for a stable order, and its layer.
type Candidate = { urgency: number; key: number; flusher: LayerFlush; chunk: Chunk }
let flushers = new Set<LayerFlush>()
let flushPending = false
let flushHooked = false

function scheduleFlush(): void {
  if (flushPending) return
  flushPending = true
  RESOLVED.then(() => flushLayers(false))
  if (flushHooked) return
  flushHooked = true
  runWithOwner(null, () =>
    onBeforeRender(
      () => {
        if (flushPending) flushLayers(true)
      },
      { publish: true },
    ),
  )
}

// `inFrame`: the frame's publish pass (the bakes join this frame's GPU
// work), else the microtask between frames (they join the next frame's).
function flushLayers(inFrame: boolean): void {
  if (!flushPending) return
  flushPending = false
  flushInFrame = inFrame
  // The residency pass runs at the flush; its events arrive with the next
  // frame, which the flush requests when something changed.
  let moved = false
  for (let f of flushers) if (f.takeViewsDirty()) moved = true
  if (moved) spatial.flush()
  let candidates: Candidate[] = []
  for (let f of flushers) f.prepare(candidates)
  // Most urgent first; ties by chunk key, so the order is the same every
  // frame.
  candidates.sort((a, b) => a.urgency - b.urgency || a.key - b.key)
  let i = 0
  for (; i < candidates.length && bakesSpent() < bakeBudget; i++) candidates[i]!.flusher.bake(candidates[i]!.chunk)
  bakesWaiting = candidates.length - i
  if (bakesWaiting > 0) {
    // The rest bake next frame: the flush stays pending for its publish
    // pass, and the frame is demanded (the loop is demand-driven; a
    // still picture would otherwise never bake them).
    flushPending = true
    requestFrame()
  }
  for (let f of flushers) f.sync()
}

/** The signed distance `d` folded into the nearest image of a period. */
function wrapDelta(d: number, period: number): number {
  return d - Math.round(d / period) * period
}

/** The greatest common divisor of two positive integers. */
function gcd(a: number, b: number): number {
  while (b !== 0) [a, b] = [b, a % b]
  return a
}

/** The pixel size of a frame on its atlas. */
function frameSize(frame: Frame, atlas: Atlas): [number, number] {
  return [Math.abs(frame.u1 - frame.u0) * atlas.width, Math.abs(frame.v1 - frame.v0) * atlas.height]
}

// The orthographic view a view pushes to the gate: world pixels to clip
// through the camera mapping (projectCamera) with the viewport's center at
// the view origin, so the gate's eye (the point enters are paced by) is
// the viewport center, and clip x/y spanning the viewport. Column-major,
// the core's matrix layout; the plane is z = 0 and the view direction -z.
function viewMatrices(camera: CameraState, width: number, height: number, view: Float32Array, proj: Float32Array): void {
  let zoom = camera.zoom
  let c = Math.cos(camera.rotation)
  let s = Math.sin(camera.rotation)
  view.fill(0)
  view[0] = zoom * c
  view[1] = zoom * s
  view[4] = -zoom * s
  view[5] = zoom * c
  view[10] = 1
  view[12] = camera.pivotX - width / 2 - zoom * (c * camera.x - s * camera.y)
  view[13] = camera.pivotY - height / 2 - zoom * (s * camera.x + c * camera.y)
  view[15] = 1
  proj.fill(0)
  proj[0] = 2 / width
  proj[5] = -2 / height
  proj[10] = -1
  proj[15] = 1
}

/**
 * Create a baked tile layer of `tileW` x `tileH` pixel cells, every cell
 * drawing one atlas frame at the frame's own size, baked into chunk
 * textures the residency gate brings and takes as its views move, and
 * shown through views as a few quads (`createView`, `<TileLayer>`).
 * Cells are unbounded; a world
 * finite by design passes `bounds`, a torus `wrap`. `atlases` is the list
 * of tilesets the cells draw from, bound together in every chunk bake
 * (see createSpriteLayer). Disposed automatically with the owning
 * reactive scope (opt out with `{ autoFree: false }`); the atlases are
 * NOT owned - dispose them yourself.
 */
export function createTileLayer(tileW: number, tileH: number, atlases: Atlas[], opts?: TileLayerOptions): TileLayer {
  let atlasIndex = checkAtlases("createTileLayer", atlases)
  if (!(tileW > 0 && tileH > 0)) {
    throw new Error(`createTileLayer: tile size must be positive, got ${tileW} x ${tileH}`)
  }
  let bounds: TileRect | null = opts?.bounds ?? null
  let wrap = opts?.wrap ?? null
  if (bounds !== null) checkTileRect("createTileLayer: bounds", bounds)
  if (wrap !== null && !(Number.isInteger(wrap.cols) && Number.isInteger(wrap.rows) && wrap.cols > 0 && wrap.rows > 0)) {
    throw new Error(`createTileLayer: wrap takes positive integer periods, got ${JSON.stringify(wrap)}`)
  }
  if (bounds !== null && wrap !== null) throw new Error("createTileLayer: bounds and wrap exclude each other (a torus has no edge)")
  let defaultChunkTiles = Math.max(1, Math.floor(CHUNK_TARGET_PX / Math.max(tileW, tileH)))
  let chunkTiles = opts?.chunkTiles ?? defaultChunkTiles
  if (opts?.chunkTiles === undefined && wrap !== null) {
    // The largest common divisor of the periods not above the default,
    // so the seam falls on a chunk edge.
    let common = gcd(wrap.cols, wrap.rows)
    chunkTiles = 1
    for (let d = 1; d <= Math.min(common, defaultChunkTiles); d++) if (common % d === 0) chunkTiles = d
  }
  if (!(chunkTiles > 0 && Number.isInteger(chunkTiles))) {
    throw new Error(`createTileLayer: chunkTiles must be a positive integer, got ${chunkTiles}`)
  }
  if (wrap !== null && (wrap.cols % chunkTiles !== 0 || wrap.rows % chunkTiles !== 0)) {
    throw new Error(`createTileLayer: chunkTiles ${chunkTiles} must divide the wrap periods ${wrap.cols} x ${wrap.rows}`)
  }
  let chunkW = chunkTiles * tileW
  let chunkH = chunkTiles * tileH
  let label = opts?.label ?? "tiles"
  let tint: Tint = opts?.tint ?? [1, 1, 1, 1]
  checkTint("createTileLayer", tint)
  let material = opts?.material ?? unlit()
  checkAtlases("createTileLayer", atlases, materialSamplers(material))
  let params: ShaderParams = { ...material.params, ...opts?.params }
  let shapes: StyleShape[] = (material.instanceBuffers ?? []).map((b, i) => ({ layout: b.attributes, blank: materialBlank(material, i), stride: layoutStride(b.attributes) }))
  // The frames table, copied (the caller's array may move on) and checked
  // once, plus its UVs, atlas index and pixel size as seven floats per
  // entry: an index write copies by offset instead of reading a frame
  // object per cell. Its largest frame sets the default overhang.
  let frames: Frame[] | null = opts?.frames ? opts.frames.slice() : null
  let table: Float32Array | null = null
  let tableOverhang = 0
  if (frames !== null) {
    if (frames.length > 0x1fffffff) throw new Error(`createTileLayer: a frames table holds at most 2^29 frames, got ${frames.length}`)
    table = new Float32Array(frames.length * TABLE_FLOATS)
    for (let i = 0; i < frames.length; i++) {
      let f = frames[i]!
      if (!isFrame(f)) throw new Error(`createTileLayer: frames[${i}] is not a frame, got ${JSON.stringify(f)}`)
      let atlas = frameIndex(`createTileLayer: frames[${i}]`, atlasIndex, f)
      let [fw, fh] = frameSize(f, atlases[atlas]!)
      let at = i * TABLE_FLOATS
      table[at] = f.u0
      table[at + 1] = f.v0
      table[at + 2] = f.u1
      table[at + 3] = f.v1
      table[at + 4] = atlas
      table[at + 5] = fw
      table[at + 6] = fh
      // The frame as it lies: a transposed non-square frame covers fh x
      // fw and needs an explicit `overhang` (the write throws otherwise).
      tableOverhang = Math.max(tableOverhang, fw - tileW, fh - tileH)
    }
  }
  let overhang = opts?.overhang ?? Math.max(0, Math.ceil(tableOverhang))
  if (!(Number.isFinite(overhang) && overhang >= 0)) throw new Error(`createTileLayer: overhang must be a non-negative number, got ${overhang}`)
  if (opts?.overhang !== undefined && tableOverhang > overhang) {
    throw new Error(`createTileLayer: the frames table overhangs the cell by ${tableOverhang} px, more than overhang ${overhang}`)
  }
  // The chunk target in world pixels: the chunk plus the margin all round.
  let targetW = chunkW + 2 * overhang
  let targetH = chunkH + 2 * overhang
  if (targetW > limits.maxTextureSize || targetH > limits.maxTextureSize) {
    throw new Error(`createTileLayer: chunk target ${targetW} x ${targetH} exceeds maxTextureSize ${limits.maxTextureSize}; lower chunkTiles`)
  }
  let density = opts?.density ?? 1
  checkOversample("createTileLayer", density, targetW, targetH, "density")
  // Pinned by the option (and by setDensity), else the views' pick.
  let pinned = opts?.density !== undefined
  let maxDensity = opts?.maxDensity
  if (maxDensity !== undefined && !(Number.isInteger(maxDensity) && maxDensity >= 1)) {
    throw new Error(`createTileLayer: maxDensity must be a positive integer, got ${maxDensity}`)
  }
  let mipmap = opts?.mipmap ?? true
  let thrash = thrashSentinel(`tile layer "${label}"`, "density", "maxDensity")
  let enter = opts?.enter ?? DEFAULT_ENTER_CHUNKS * Math.max(chunkW, chunkH)
  let leave = opts?.leave ?? Math.max(enter, DEFAULT_LEAVE_CHUNKS * Math.max(chunkW, chunkH))
  if (!(Number.isFinite(enter) && enter >= 0 && Number.isFinite(leave) && leave >= enter)) {
    throw new Error(`createTileLayer: enter must be non-negative and leave at least enter, got ${enter} and ${leave}`)
  }
  // A chunk's cost to the residency limits: its target's texels at the
  // current density, the mip chain included (the grid's weight,
  // re-pushed on setDensity).
  let chunkTexels = (n: number) => Math.ceil(targetW * n * (targetH * n) * (mipmap ? MIP_CHAIN_FACTOR : 1))
  let maxTexels = opts?.maxTexels
  if (maxTexels !== undefined && !(Number.isFinite(maxTexels) && maxTexels >= chunkTexels(density))) {
    throw new Error(`createTileLayer: maxTexels must hold at least one chunk (${chunkTexels(density)} texels at density ${density}), got ${maxTexels}`)
  }
  let retain = opts?.retain ?? "records"
  if (retain !== "records" && retain !== "none") throw new Error(`createTileLayer: retain must be "records" or "none", got ${JSON.stringify(retain)}`)
  let fill = opts?.fill
  let perChunk = chunkTiles * chunkTiles
  // The one pipeline (and unit quad) every chunk target draws with:
  // the material's, resolved once per layer, however many chunks
  // allocate.
  let gpu = material.pipeline(atlases, [RECORD_ATTRIBUTES], label)
  let textures = { ...atlasBindings(atlases.map(a => a.texture)), ...material.textures }
  // Every chunk target's uniforms are the same: records are chunk-local,
  // the camera sits at minus the margin, and bake targets pin the camera
  // rotation to identity (the tile camera rotates the COMPOSITE, never
  // the bake).
  let targetParams = (): ShaderParams => ({ uViewport: [targetW, targetH], uCamera: [-overhang, -overhang, 1, 1], uCameraRot: [1, 0, 0, 0], uTint: tint, ...params })
  // The pool: the layer's own, or the one given, with the layer's two
  // kinds registered on it either way (TARGET_KIND, BUFFER_KIND).
  let ownPool = opts?.pool === undefined
  let pool: ResourcePool = opts?.pool ?? createResourcePool({ cap: POOL_CHUNKS * (2 + shapes.length), autoFree: false })
  pool.destroyWith(TARGET_KIND, freeTarget)
  pool.destroyWith(BUFFER_KIND, freeBuffer)
  // Sampling is creation-time state on a target, so a chain and a
  // chainless target are two classes.
  let targetClass = `${TARGET_KIND}:${gpu.pipeline}:${mipmap ? "mip" : "flat"}`
  let bufferClass = (bytes: number) => `${BUFFER_KIND}:${bytes}`
  // The views' pipeline, shared by every tile layer.
  let quadPipe = tilePipeline()

  let disposed = false
  // A view's camera changed since the last flush: the spatial core is
  // flushed there, once, so the residency pass runs in the frame's
  // publish pass beside the bakes (as a sprite layer's sync flushes it),
  // never once per camera write.
  let viewsDirty = false
  // chunkKey -> chunk: every chunk the layer knows (written or resident).
  let chunks = new Map<number, Chunk>()
  // The wanted chunks (entered, not left): what every per-resident walk
  // covers, so a world of ten thousand written chunks costs nothing per
  // enter, leave or view sync beyond the chunks on screen.
  let resident = new Set<Chunk>()
  // The chunks with a bake pending: the fresh ones bake at the next
  // flush, the drawn ones when the bake budget reaches them.
  let dirty = new Set<Chunk>()
  let views = new Set<TileView>()
  // usedRect's cache: undefined when a write invalidated it.
  let used: TileRect | null | undefined = null

  // A chunk's texture came or went: every view's entry set is stale.
  let markViews = () => {
    for (let tv of views) tv.dirty = true
    scheduleFlush()
  }
  // Publish and bake one chunk. A re-bake of a chunk the views have drawn
  // swaps to a retired set from the pool first: the frames in flight
  // still sample its target and its bake still reads its buffers, and a
  // write into either makes the driver drain the GPU behind them (27 ms
  // on an Adreno 610, the hitch of
  // okf/backlog/2d-tile-world-frame-hitches.md); the views rebind to the
  // new set. Then the records go up if they changed, and the pass runs.
  let bakeChunk = (chunk: Chunk) => {
    dirty.delete(chunk)
    chunk.dirty = false
    if (!chunk.fresh) {
      giveResources(chunk)
      takeResources(chunk)
      for (let tv of views) tv.dirty = true
    }
    if (chunk.wrote) {
      chunk.wrote = false
      let out = beginBufferWrite(chunk.buffer!)
      out.set(chunk.records!)
      endBufferWrite(chunk.buffer!, chunk.records!.byteLength)
      for (let style of chunk.styles) writeBuffer(style.buffer!, style.bytes)
    }
    renderTarget(chunk.texture!)
    chunk.fresh = false
    spendBake()
  }
  // How needed a chunk's re-bake is, the gate's measure (RegionMargins
  // urgency): the distance from the chunk's center to the nearest view's
  // eye (the viewport center in world pixels; under wrap, to the nearest
  // image of the chunk) in `leave`s, dimensionless so layers in different
  // units compare; the raw distance under a zero band.
  let urgencyOf = (chunk: Chunk): number => {
    let cx = chunk.x + chunk.width / 2
    let cy = chunk.y + chunk.height / 2
    let nearest = Infinity
    for (let tv of views) {
      if (tv.key === null) continue
      let { width, height } = tv.view.size()
      let [ex, ey] = unprojectCamera(tv.view.camera(), width / 2, height / 2)
      let dx = cx - ex
      let dy = cy - ey
      if (wrap !== null) {
        dx = wrapDelta(dx, wrap.cols * tileW)
        dy = wrapDelta(dy, wrap.rows * tileH)
      }
      nearest = Math.min(nearest, Math.hypot(dx, dy))
    }
    return leave > 0 ? nearest / leave : nearest
  }
  // The layer's part in the shared flush (flushLayers).
  let flusher: LayerFlush = {
    takeViewsDirty() {
      if (!viewsDirty) return false
      viewsDirty = false
      return true
    },
    prepare(out) {
      if (disposed) return
      for (let chunk of dirty) {
        // A chunk that left before its bake: nothing to bake (the cancel).
        if (chunk.texture === null || chunk.buffer === null || chunk.records === null) {
          dirty.delete(chunk)
          chunk.dirty = false
          continue
        }
        if (chunk.fresh) bakeChunk(chunk)
        else out.push({ urgency: urgencyOf(chunk), key: chunk.key, flusher, chunk })
      }
    },
    bake: bakeChunk,
    sync() {
      if (disposed) return
      for (let tv of views) if (tv.dirty) syncView(tv)
    },
  }
  flushers.add(flusher)

  // The images a view wants, in draw order: one per resident chunk with
  // a texture, or under wrap one per image of it whose chunk indices
  // fall in the view's range; row-major by world row then column, so a
  // lower row's overhang draws over the row above and a wider frame's
  // right overhang under its right neighbour - the leaf composite's
  // order, exact across the seam at any view size.
  let wantedImages = (tv: TileView): Image[] => {
    let out: Image[] = []
    for (let chunk of resident) {
      if (chunk.texture === null) continue
      if (wrap === null) {
        out.push({ key: chunk.key, col: chunk.chunkCol, row: chunk.chunkRow, chunk })
        continue
      }
      if (tv.range === null) continue
      let [c0, c1, r0, r1] = tv.range
      let pc = wrap.cols / chunkTiles
      let pr = wrap.rows / chunkTiles
      let iy0 = Math.ceil((r0 - chunk.chunkRow) / pr)
      let iy1 = Math.floor((r1 - chunk.chunkRow) / pr)
      let ix0 = Math.ceil((c0 - chunk.chunkCol) / pc)
      let ix1 = Math.floor((c1 - chunk.chunkCol) / pc)
      for (let iy = iy0; iy <= iy1; iy++) {
        for (let ix = ix0; ix <= ix1; ix++) {
          let col = chunk.chunkCol + ix * pc
          let row = chunk.chunkRow + iy * pr
          out.push({ key: chunkKey(col, row), col, row, chunk })
        }
      }
    }
    return out.sort((a, b) => a.row - b.row || a.col - b.col)
  }
  // Bring a view's entries to the wanted set: add the missing (binding
  // the chunk's current target), rebind one whose chunk came back from
  // the pool under another id, remove the gone, and re-issue the order.
  let syncView = (tv: TileView) => {
    tv.dirty = false
    let wanted = wantedImages(tv)
    let keep = new Set<number>()
    let order: DrawId[] = []
    for (let im of wanted) {
      let texture = im.chunk.texture!
      let entry = tv.entries.get(im.key)
      if (entry === undefined) {
        let draw = addDraw(tv.view.texture, quadPipe.pipeline, { uChunk: [im.col, im.row] }, { textures: { [CHUNK_SAMPLER]: texture }, buffers: [quadPipe.quad], vertexCount: 4, label: `${label}-chunk` })
        entry = { draw, texture }
        tv.entries.set(im.key, entry)
      } else if (entry.texture !== texture) {
        setDrawTextures(tv.view.texture, entry.draw, { [CHUNK_SAMPLER]: texture })
        entry.texture = texture
      }
      keep.add(im.key)
      order.push(entry.draw)
    }
    for (let [key, entry] of tv.entries) {
      if (keep.has(key)) continue
      removeDraw(tv.view.texture, entry.draw)
      tv.entries.delete(key)
    }
    setDrawOrder(tv.view.texture, order)
  }
  // Under wrap, the chunk-index range of the images intersecting the
  // view's world box (its corners unprojected): an image at chunk column
  // C spans [C * chunkW - overhang, (C + 1) * chunkW + overhang) in x,
  // likewise in y. Four integers: the image set's whole camera
  // dependence, so a scroll inside the same range changes no entry.
  let viewRange = (tv: TileView): [number, number, number, number] => {
    let cam = tv.view.camera()
    let { width, height } = tv.view.size()
    let minX = Infinity
    let minY = Infinity
    let maxX = -Infinity
    let maxY = -Infinity
    for (let [sx, sy] of [
      [0, 0],
      [width, 0],
      [0, height],
      [width, height],
    ]) {
      let [wx, wy] = unprojectCamera(cam, sx!, sy!)
      minX = Math.min(minX, wx)
      maxX = Math.max(maxX, wx)
      minY = Math.min(minY, wy)
      maxY = Math.max(maxY, wy)
    }
    return [Math.floor((minX - overhang) / chunkW), Math.ceil((maxX + overhang) / chunkW) - 1, Math.floor((minY - overhang) / chunkH), Math.ceil((maxY + overhang) / chunkH) - 1]
  }
  let touch = (chunk: Chunk, wroteRecords = true) => {
    if (wroteRecords) chunk.wrote = true
    if (chunk.dirty) return
    chunk.dirty = true
    dirty.add(chunk)
    scheduleFlush()
  }

  // The chunk at canonical chunk indices, made on first contact: no
  // records, no GPU - a name and a rect.
  let chunkAt = (chunkCol: number, chunkRow: number): Chunk => {
    let key = chunkKey(chunkCol, chunkRow)
    let chunk = chunks.get(key)
    if (chunk) return chunk
    chunk = {
      key,
      chunkCol,
      chunkRow,
      col: chunkCol * chunkTiles,
      row: chunkRow * chunkTiles,
      cols: chunkTiles,
      rows: chunkTiles,
      x: chunkCol * chunkW,
      y: chunkRow * chunkH,
      width: chunkW,
      height: chunkH,
      texture: null,
      records: null,
      buffer: null,
      styles: [],
      count: 0,
      wanted: false,
      dirty: false,
      wrote: false,
      fresh: false,
    }
    chunks.set(key, chunk)
    return chunk
  }
  // A chunk's records come to exist at its first cell: every style slot
  // starts blank (the material's instanceStyle), so an unwritten cell
  // shows the material's default.
  let ensureRecords = (chunk: Chunk): Float32Array => {
    if (chunk.records !== null) return chunk.records
    chunk.records = new Float32Array(perChunk * INSTANCE_FLOATS)
    chunk.styles = shapes.map(shape => {
      let bytes = new Uint8Array(perChunk * shape.stride)
      for (let slot = 0; slot < perChunk; slot++) bytes.set(shape.blank, slot * shape.stride)
      let view = new DataView(bytes.buffer)
      return { bytes, buffer: null, fields: layoutFields(shape.layout, () => view), floats: isFloatLayout(shape.layout) ? new Float32Array(bytes.buffer) : null }
    })
    return chunk.records
  }
  // The chunk's GPU side from the pool (or created): its records buffer,
  // its style buffers and a target at the current density, params and
  // draw set; a fresh set, never drawn, with the records still to upload.
  // The pool hands out only ids retired from the frames in flight
  // (createResourcePool's `retire`), so nothing here is read by a frame
  // the GPU is still on. Every chunk resource carries the LAYER's label,
  // so the inventory's group under it (/gpu `groups`) is the layer's
  // footprint.
  let takeResources = (chunk: Chunk) => {
    let bytes = chunk.records!.byteLength
    let buffer = (pool.take(bufferClass(bytes)) as BufferId | undefined) ?? createBuffer(bytes, { label, autoFree: false })
    chunk.buffer = buffer
    for (let style of chunk.styles) {
      style.buffer = (pool.take(bufferClass(style.bytes.byteLength)) as BufferId | undefined) ?? createBuffer(style.bytes.byteLength, { label, autoFree: false })
    }
    let buffers = [gpu.quad, buffer, ...chunk.styles.map(s => s.buffer!)]
    let w = targetW * density
    let h = targetH * density
    let texture = pool.take(targetClass) as TextureId | undefined
    if (texture !== undefined) {
      if (targetSizes.get(texture) !== w * h) setTargetSize(texture, w, h)
      setTargetParams(texture, targetParams())
      setDraw(texture, { buffers, instanceCount: perChunk })
    } else {
      // Linear with the chain: the views resample the bake at any zoom;
      // hard pixels belong to the atlas sampler.
      texture = createShaderTarget(gpu.pipeline, w, h, targetParams(), {
        label,
        buffers,
        vertexCount: 4,
        instanceCount: perChunk,
        textures,
        clearColor: opts?.chunkClearColor ?? [0, 0, 0, 0],
        mipmap,
        render: "manual",
        autoFree: false,
      })
    }
    targetSizes.set(texture, w * h)
    chunk.texture = texture
    chunk.fresh = true
    chunk.wrote = true
  }
  // Give the chunk's GPU side back to the pool, which keeps it from the
  // next take until the frames in flight have retired.
  let giveResources = (chunk: Chunk) => {
    pool.give(targetClass, chunk.texture!)
    pool.give(bufferClass(chunk.records!.byteLength), chunk.buffer!)
    for (let style of chunk.styles) {
      pool.give(bufferClass(style.bytes.byteLength), style.buffer!)
      style.buffer = null
    }
    chunk.texture = null
    chunk.buffer = null
    chunk.fresh = false
  }
  // Take the chunk's GPU side and queue its upload and bake; the views
  // learn of the texture.
  let acquire = (chunk: Chunk) => {
    if (chunk.texture !== null || chunk.records === null) return
    takeResources(chunk)
    touch(chunk)
    markViews()
  }
  // Give the chunk's GPU side back; its records stay (the caller drops
  // them under retain "none").
  let release = (chunk: Chunk) => {
    if (chunk.texture === null) return
    giveResources(chunk)
    dirty.delete(chunk)
    chunk.dirty = false
    chunk.wrote = false
    markViews()
  }
  // The chunk's object is let go of when nothing remains to remember: no
  // records, not wanted.
  let forget = (chunk: Chunk) => {
    if (chunk.wanted || chunk.records !== null) return
    chunks.delete(chunk.key)
  }

  // The gate's two verbs.
  let enterChunk = (i: number, j: number) => {
    if (disposed) return
    let chunk = chunkAt(i, j)
    if (chunk.wanted) return
    chunk.wanted = true
    resident.add(chunk)
    if (chunk.records === null && fill !== undefined) fill(chunk)
    if (chunk.count > 0) acquire(chunk)
    layer.onEnter?.(chunk)
  }
  let leaveChunk = (i: number, j: number) => {
    if (disposed) return
    let chunk = chunks.get(chunkKey(i, j))
    if (!chunk || !chunk.wanted) return
    chunk.wanted = false
    resident.delete(chunk)
    release(chunk)
    if (retain === "none" && chunk.records !== null) {
      chunk.records = null
      chunk.styles = []
      chunk.count = 0
      used = undefined
    }
    forget(chunk)
    layer.onLeave?.(chunk)
  }
  let grid = spatial.createGrid({
    origin: [0, 0, 0],
    cell: [chunkW, chunkH, 0],
    enter,
    leave,
    bounds:
      bounds === null
        ? undefined
        : [floorDiv(bounds.col, chunkTiles), floorDiv(bounds.row, chunkTiles), 0, floorDiv(bounds.col + bounds.cols - 1, chunkTiles), floorDiv(bounds.row + bounds.rows - 1, chunkTiles), 0],
    wrap: wrap === null ? undefined : [wrap.cols / chunkTiles, wrap.rows / chunkTiles, 0],
    weight: chunkTexels(density),
    limit: maxTexels,
    label,
  })
  handlers.set(grid, { enter: enterChunk, leave: leaveChunk })
  subscribe()

  // A cell's canonical coordinates under wrap (identity otherwise).
  let canonCol = (col: number) => (wrap === null ? col : mod(col, wrap.cols))
  let canonRow = (row: number) => (wrap === null ? row : mod(row, wrap.rows))
  let slot = (col: number, row: number): number => slotOf(col, row, chunkTiles, INSTANCE_FLOATS)
  // A frame's size against the cell plus the margin (throws - the dev
  // policy): the bake would clip it otherwise.
  let checkOverhang = (verb: string, fw: number, fh: number, transpose: boolean): void => {
    let [footW, footH] = footprint(fw, fh, transpose)
    if (footW - tileW > overhang || footH - tileH > overhang) {
      throw new Error(`${verb}: a ${footW} x ${footH} frame exceeds the ${tileW} x ${tileH} cell by more than the layer's overhang ${overhang}`)
    }
  }
  // Write one cell's record: the quad of the frame's size placed with its
  // footprint's bottom-left on the cell's bottom-left (chunk-local), the
  // frame's UVs swapped and the quad turned for the cell's symmetry, its
  // atlas index, and the tint - the one given, else the default when the
  // cell comes up from empty (a re-set keeps its tint: absent keys keep
  // their values, like every options object here).
  let writeCell = (
    chunk: Chunk,
    r: Float32Array,
    at: number,
    localCol: number,
    localRow: number,
    u0: number,
    v0: number,
    u1: number,
    v1: number,
    atlas: number,
    fw: number,
    fh: number,
    flipX: boolean,
    flipY: boolean,
    transpose: boolean,
    cellTint: Tint | undefined,
  ): void => {
    let fresh = r[at + 2] === 0
    if (fresh) chunk.count++
    let [footW, footH] = footprint(fw, fh, transpose)
    let sym = cellSymmetry(flipX, flipY, transpose)
    r[at] = localCol * tileW + footW / 2
    r[at + 1] = (localRow + 1) * tileH - footH / 2
    r[at + 2] = fw
    r[at + 3] = fh
    writeFrame(r, at + 4, u0, v0, u1, v1, sym.flipU, sym.flipV)
    r[at + ROT_FIELD_OFFSET] = sym.rot
    r[at + ATLAS_FIELD_OFFSET] = atlas
    if (cellTint !== undefined) {
      r[at + 9] = cellTint[0]
      r[at + 10] = cellTint[1]
      r[at + 11] = cellTint[2]
      r[at + 12] = cellTint[3]
    } else if (fresh) {
      r[at + 9] = 1
      r[at + 10] = 1
      r[at + 11] = 1
      r[at + 12] = 1
    }
  }
  // Clear one cell: a zero-size quad draws nothing (the tint stays, and
  // a later set from empty resets it; the style stays the same way).
  let clearCell = (chunk: Chunk, r: Float32Array, at: number): void => {
    if (r[at + 2] !== 0) chunk.count--
    r[at + 2] = 0
    r[at + 3] = 0
  }
  // A cell write's style, checked before any cell changes: the material
  // must declare an instance buffer, and the record must be its length
  // (the encode throws on a wrong count).
  let checkStyle = (verb: string, style: ArrayLike<number> | undefined): void => {
    if (style === undefined) return
    if (shapes.length === 0) throw new Error(`${verb}: the layer's material declares no instance buffer (no style record)`)
  }
  // Write a cell's style record into its chunk's first style stream.
  let writeStyle = (chunk: Chunk, slot: number, style: ArrayLike<number>, verb: string): void => {
    let s = chunk.styles[0]!
    writeRecord(shapes[0]!.layout, s.fields, s.floats, slot, style, verb)
  }
  // After a chunk's cells changed: a wanted chunk with content holds GPU
  // and re-bakes, one emptied gives its GPU back; the used rect is stale.
  let changed = (chunk: Chunk) => {
    used = undefined
    if (chunk.wanted) {
      if (chunk.count > 0) {
        if (chunk.texture === null) acquire(chunk)
        else touch(chunk)
      } else release(chunk)
    }
  }

  // Re-bake every resident chunk at density `n`: each resident chunk is
  // marked for the flush (the bake budget paces them, nearest a view
  // first), where a drawn chunk swaps to a target at the new size
  // (takeResources sizes it) and a never-drawn one, resized here in
  // place, bakes into its own; pooled targets resize on their next take.
  // The limits weigh a chunk at its new size.
  let applyDensity = (n: number) => {
    if (n === density) return
    thrash()
    density = n
    for (let chunk of resident) {
      if (chunk.texture === null) continue
      if (chunk.fresh) {
        setTargetSize(chunk.texture, targetW * n, targetH * n)
        targetSizes.set(chunk.texture, targetW * n * (targetH * n))
      }
      touch(chunk, false)
    }
    spatial.setGridWeight(grid, chunkTexels(n))
  }
  // Each view's wish for the density: its target texels per world pixel
  // (oversample times zoom).
  let wishes = new Map<TileView, number>()
  // The views' pick: the largest wish, rounded up, bounded by the cap
  // and the window's pixel count per chunk, with the shrink hysteresis
  // (oversample-math.ts); under `maxTexels` the pick is clamped so a
  // chunk always fits the budget (the fit rounds up and could not
  // otherwise). Nothing to pick from before the first view reports.
  let resolve = () => {
    if (pinned || wishes.size === 0) return
    let scale = 0
    for (let w of wishes.values()) scale = Math.max(scale, w)
    // Untracked: a view's camera write runs inside an effect, and the
    // window follows through the components' layout, not through here.
    let win = untrack(windowSize)
    let ds = untrack(displayScale)
    let budget = win.width * ds * (win.height * ds)
    if (maxTexels !== undefined) budget = Math.min(budget, maxTexels)
    let n = pickOversample(density, scale, targetW, targetH, budget, limits.maxTextureSize, maxDensity)
    if (n === null) return
    if (maxTexels !== undefined) n = Math.max(1, Math.min(n, Math.floor(Math.sqrt(maxTexels / chunkTexels(1)))))
    applyDensity(n)
  }
  // A view's camera, size or oversample changed: its gate view, its
  // density wish and, under wrap, its image range follow.
  let viewChanged = (tv: TileView) => {
    if (tv.key === null || disposed) return
    let cam = tv.view.camera()
    let { width, height } = tv.view.size()
    viewMatrices(cam, width, height, gateView, gateProj)
    spatial.setView(tv.key, gateView, gateProj)
    viewsDirty = true
    scheduleFlush()
    wishes.set(tv, tv.view.oversample * cam.zoom)
    resolve()
    if (wrap === null) return
    let range = viewRange(tv)
    let last = tv.range
    if (last !== null && last[0] === range[0] && last[1] === range[1] && last[2] === range[2] && last[3] === range[3]) return
    tv.range = range
    tv.dirty = true
  }
  let gateView = new Float32Array(16)
  let gateProj = new Float32Array(16)

  let layer: TileLayer = {
    atlases,
    material,
    tileW,
    tileH,
    chunkTiles,
    chunkW,
    chunkH,
    overhang,
    bounds,
    wrap,
    get density() {
      return density
    },
    mipmap,
    setDensity(n) {
      if (disposed) return
      if (n === null) {
        pinned = false
        resolve()
        return
      }
      checkOversample("setDensity", n, targetW, targetH, "density")
      if (maxTexels !== undefined && maxTexels < chunkTexels(n)) {
        throw new Error(`setDensity: maxTexels ${maxTexels} holds no chunk at density ${n} (${chunkTexels(n)} texels); raise it or pin lower`)
      }
      pinned = true
      applyDensity(n)
    },
    createView(vopts) {
      if (disposed) throw new Error("createView: the layer is disposed")
      let tv: TileView = { view: undefined as unknown as ViewHandle, key: spatial.allocViewKey(), entries: new Map(), range: null, dirty: true }
      tv.view = createViewShell(vopts, {
        label,
        pick: () => [],
        cameraParams: cam => tileCameraParams(cam, chunkW, chunkH),
        // uTint is the VIEW's tint over the bake (identity until a
        // per-view tint exists); the layer's tint is baked into the chunks.
        createTarget: (w, h, params, o) => createDrawTarget(w, h, { ...params, uChunkRect: [chunkW, chunkH, overhang, 0], uTint: [1, 1, 1, 1] }, { clearColor: o.clearColor, label: o.label, autoFree: false }),
        changed: () => viewChanged(tv),
        disposing() {
          views.delete(tv)
          wishes.delete(tv)
          // The entries die with the target.
          tv.entries.clear()
          if (tv.key !== null && !disposed) {
            spatial.setView(tv.key, null)
            viewsDirty = true
            scheduleFlush()
            resolve()
          }
          tv.key = null
        },
      })
      views.add(tv)
      viewChanged(tv)
      return tv.view
    },
    resident() {
      return [...resident]
    },
    setTile(col, row, frame, opts) {
      if (disposed) return
      checkCell("setTile", col, row, bounds)
      col = canonCol(col)
      row = canonRow(row)
      let chunk = chunkAt(floorDiv(col, chunkTiles), floorDiv(row, chunkTiles))
      let at = slot(col, row)
      if (frame === null) {
        // Clearing a cell no records hold is a no-op, not an allocation.
        if (chunk.records === null) {
          forget(chunk)
          return
        }
        clearCell(chunk, chunk.records, at)
      } else {
        if (!isFrame(frame)) throw new Error(`setTile: not a frame, got ${JSON.stringify(frame)}`)
        if (opts?.tint !== undefined) checkTint("setTile", opts.tint)
        checkStyle("setTile", opts?.style)
        let atlas = frameIndex("setTile", atlasIndex, frame)
        let [fw, fh] = frameSize(frame, atlases[atlas]!)
        let transpose = opts?.transpose ?? false
        checkOverhang("setTile", fw, fh, transpose)
        let r = ensureRecords(chunk)
        writeCell(chunk, r, at, mod(col, chunkTiles), mod(row, chunkTiles), frame.u0, frame.v0, frame.u1, frame.v1, atlas, fw, fh, opts?.flipX ?? false, opts?.flipY ?? false, transpose, opts?.tint)
        if (opts?.style !== undefined) writeStyle(chunk, at / INSTANCE_FLOATS, opts.style, "setTile")
      }
      changed(chunk)
    },
    setTiles(col, row, w, h, cells, opts) {
      if (disposed) return
      checkRect("setTiles", col, row, w, h, bounds)
      if (wrap !== null && (w > wrap.cols || h > wrap.rows)) throw new Error(`setTiles: a ${w} x ${h} rect exceeds the wrap periods ${wrap.cols} x ${wrap.rows}`)
      if (cells.length !== w * h) throw new Error(`setTiles: a ${w} x ${h} rect takes ${w * h} cells, got ${cells.length}`)
      if (opts?.tint !== undefined) checkTint("setTiles", opts.tint)
      checkStyle("setTiles", opts?.style)
      let cellTint = opts?.tint
      let cellStyle = opts?.style
      let optFlipX = opts?.flipX ?? false
      let optFlipY = opts?.flipY ?? false
      let optTranspose = opts?.transpose ?? false
      // The two forms: indices into the table, or frames. Every entry is
      // checked (and a frame resolved to its atlas and measured) before
      // the first write, so a bad one changes nothing.
      let idx = typeof cells[0] === "number" ? (cells as ArrayLike<number>) : null
      let objs = idx === null ? (cells as ArrayLike<Frame | null>) : null
      // Per frame-form entry: the atlas index and the frame's pixel size.
      let objMeta: Float32Array | null = null
      if (idx !== null) {
        if (frames === null) throw new Error("setTiles: index cells need a frames table (createTileLayer's frames option)")
        let count = frames.length
        for (let i = 0; i < idx.length; i++) {
          let k = idx[i]!
          if (isClearIndex(k)) continue
          if (!Number.isInteger(k)) throw new Error(`setTiles: index ${k} at cell ${i} is not an integer`)
          let index = indexOf(k)
          if (index >= count) throw new Error(`setTiles: index ${index} at cell ${i} outside the ${count}-frame table`)
          let b = index * TABLE_FLOATS
          checkOverhang(`setTiles: cell ${i}`, table![b + 5]!, table![b + 6]!, flagsOf(k).transpose)
        }
      } else {
        objMeta = new Float32Array(objs!.length * 3)
        for (let i = 0; i < objs!.length; i++) {
          let f = objs![i]
          if (f === null || f === undefined) continue
          if (!isFrame(f)) throw new Error(`setTiles: cell ${i} is neither a frame nor null, got ${JSON.stringify(f)}`)
          let atlas = frameIndex(`setTiles: cell ${i}`, atlasIndex, f)
          let [fw, fh] = frameSize(f, atlases[atlas]!)
          checkOverhang(`setTiles: cell ${i}`, fw, fh, optTranspose)
          objMeta[i * 3] = atlas
          objMeta[i * 3 + 1] = fw
          objMeta[i * 3 + 2] = fh
        }
      }
      // Chunk by chunk over the rect: each chunk's slice of the rect is a
      // sub-rect walked row by row with the record offset advancing, one
      // dirty mark at the end. A chunk the slice only clears never
      // allocates records. Under wrap the rect's origin is canonical and
      // a slice past the seam lands in the canonical chunk.
      col = canonCol(col)
      row = canonRow(row)
      eachChunkSlice(col, row, w, h, chunkTiles, (cc, cr, colA, colB, rowA, rowB) => {
        let chunk = chunkAt(wrap === null ? cc : mod(cc, wrap.cols / chunkTiles), wrap === null ? cr : mod(cr, wrap.rows / chunkTiles))
        let r = chunk.records
        for (let y = rowA; y < rowB; y++) {
          let at = slot(colA, y)
          let i = (y - row) * w + (colA - col)
          for (let x = colA; x < colB; x++, at += INSTANCE_FLOATS, i++) {
            let u0: number
            let v0: number
            let u1: number
            let v1: number
            let atlas: number
            let fw: number
            let fh: number
            let flipX = optFlipX
            let flipY = optFlipY
            let transpose = optTranspose
            if (idx !== null) {
              let k = idx[i]!
              if (isClearIndex(k)) {
                if (r !== null) clearCell(chunk, r, at)
                continue
              }
              let b = indexOf(k) * TABLE_FLOATS
              u0 = table![b]!
              v0 = table![b + 1]!
              u1 = table![b + 2]!
              v1 = table![b + 3]!
              atlas = table![b + 4]!
              fw = table![b + 5]!
              fh = table![b + 6]!
              let flags = flagsOf(k)
              flipX = flags.flipX
              flipY = flags.flipY
              transpose = flags.transpose
            } else {
              let f = objs![i]
              if (f === null || f === undefined) {
                if (r !== null) clearCell(chunk, r, at)
                continue
              }
              u0 = f.u0
              v0 = f.v0
              u1 = f.u1
              v1 = f.v1
              atlas = objMeta![i * 3]!
              fw = objMeta![i * 3 + 1]!
              fh = objMeta![i * 3 + 2]!
            }
            if (r === null) r = ensureRecords(chunk)
            writeCell(chunk, r, at, mod(x, chunkTiles), mod(y, chunkTiles), u0, v0, u1, v1, atlas, fw, fh, flipX, flipY, transpose, cellTint)
            if (cellStyle !== undefined) writeStyle(chunk, at / INSTANCE_FLOATS, cellStyle, "setTiles")
          }
        }
        if (r !== null) changed(chunk)
        else forget(chunk)
      })
    },
    getTile(col, row) {
      checkCell("getTile", col, row, bounds)
      col = canonCol(col)
      row = canonRow(row)
      let chunk = chunks.get(chunkKey(floorDiv(col, chunkTiles), floorDiv(row, chunkTiles)))
      if (!chunk || chunk.records === null) return null
      let at = slot(col, row)
      let r = chunk.records
      if (r[at + 2] === 0) return null
      // The stored UVs carry the symmetry's swaps; the canonical frame is
      // the rect itself.
      return {
        texture: atlases[r[at + ATLAS_FIELD_OFFSET]!]!.texture,
        u0: Math.min(r[at + 4]!, r[at + 6]!),
        v0: Math.min(r[at + 5]!, r[at + 7]!),
        u1: Math.max(r[at + 4]!, r[at + 6]!),
        v1: Math.max(r[at + 5]!, r[at + 7]!),
      }
    },
    usedRect() {
      if (used !== undefined) return used
      let minCol = Infinity
      let minRow = Infinity
      let maxCol = -Infinity
      let maxRow = -Infinity
      for (let chunk of chunks.values()) {
        if (chunk.records === null || chunk.count === 0) continue
        let r = chunk.records
        for (let lr = 0; lr < chunkTiles; lr++) {
          for (let lc = 0; lc < chunkTiles; lc++) {
            if (r[(lr * chunkTiles + lc) * INSTANCE_FLOATS + 2] === 0) continue
            let c = chunk.col + lc
            let w = chunk.row + lr
            if (c < minCol) minCol = c
            if (c > maxCol) maxCol = c
            if (w < minRow) minRow = w
            if (w > maxRow) maxRow = w
          }
        }
      }
      used = minCol === Infinity ? null : { col: minCol, row: minRow, cols: maxCol - minCol + 1, rows: maxRow - minRow + 1 }
      return used
    },
    frames,
    setTint(next) {
      if (disposed) return
      checkTint("setTint", next)
      if (next[0] === tint[0] && next[1] === tint[1] && next[2] === tint[2] && next[3] === tint[3]) return
      tint = next
      // A drawn chunk's swap at the flush takes a target carrying the new
      // params (targetParams); a never-drawn one takes them in place.
      for (let chunk of resident) {
        if (chunk.texture === null) continue
        if (chunk.fresh) setTargetParams(chunk.texture, { uTint: tint })
        touch(chunk, false)
      }
    },
    setParams(next) {
      if (disposed) return
      checkParams("setParams", next)
      Object.assign(params, next)
      for (let chunk of resident) {
        if (chunk.texture === null) continue
        if (chunk.fresh) setTargetParams(chunk.texture, next)
        touch(chunk, false)
      }
    },
    dispose() {
      if (disposed) return
      disposed = true
      flushers.delete(flusher)
      dirty.clear()
      handlers.delete(grid)
      spatial.destroyGrid(grid)
      // The views first: their entries bind the chunk targets.
      for (let tv of [...views]) tv.view.dispose()
      // The resident chunks' GPU side is destroyed outright: a shared pool
      // is not the place for a dying layer's resources.
      for (let chunk of resident) {
        if (chunk.texture === null) continue
        freeTarget(chunk.texture)
        destroyBuffer(chunk.buffer!)
        for (let style of chunk.styles) destroyBuffer(style.buffer!)
        chunk.texture = null
        chunk.buffer = null
      }
      chunks.clear()
      resident.clear()
      if (ownPool) pool.drain()
      spatial.flush()
    },
  }

  if (opts?.autoFree !== false && getOwner()) onCleanup(() => layer.dispose())
  return layer
}
