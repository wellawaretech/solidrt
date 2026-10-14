// The baked tile layer: an unbounded grid of atlas frames rendered into
// CHUNKED `render: "manual"` targets and composited as a handful of quads -
// never a quad per tile. On tiled GPUs the budget is primitive count, so a
// 100x100 world must not be 10,000 quads per frame; baked, it is a few
// chunk textures, and scrolling is a transform on the composite (see
// <TileLayer> in components/tile-layer.tsx), never a repaint.
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
// every side, the composite overlaps leaves by that margin).
//
// Residency: the layer is a HANDLER over the core's residency gate
// (flux:spatial createGrid, the Residency section of @solidrt/3d's
// AGENTS.md): its chunk grid is a grid of regions, a viewer (addViewer; the
// component pushes its camera as one) wants the chunks near its view, and
// the gate's enter and leave events drive the GPU side. On ENTER a chunk
// takes a records buffer and a target from the shared pool
// (createResourcePool in @solidrt/core/gpu), uploads its records and bakes
// in the same frame's publish pass; on LEAVE they go back to the pool and
// the records stay on the CPU (~5 KB a chunk) - or go too under `retain:
// "none"`, where `fill` regenerates the chunk on the next enter. The gate
// decides when (hysteresis, the union over viewers, the paced budget); the
// layer never does. A layer with no viewer bakes nothing.
//
// setTile and setTiles batch; the flush (ahead of the frame's paint, or at
// a microtask outside a frame) publishes and re-bakes ONLY dirty resident
// chunks. A layer nobody edits and nobody moves over costs nothing per
// frame.
import { displayScale, getOwner, onBeforeRender, onCleanup, runWithOwner, windowSize } from "@solidrt/core"
import {
  beginBufferWrite,
  createBuffer,
  createResourcePool,
  createShaderTarget,
  destroyBuffer,
  destroyTexture,
  endBufferWrite,
  isFloatLayout,
  layoutFields,
  layoutStride,
  limits,
  renderTarget,
  setDraw,
  setTargetParams,
  setTargetSize,
  writeBuffer,
  writeRecord,
} from "@solidrt/core/gpu"
import type { AttributeAccess, BufferId, FilterMode, ResourcePool, ShaderParams, TextureId, VertexAttribute } from "@solidrt/core/gpu"
import * as spatial from "flux:spatial"
import type { GridId, ViewKey } from "flux:spatial"
import { on } from "sol:events"
import { checkAtlases, frameIndex } from "./atlas.ts"
import type { Atlas } from "./atlas.ts"
import { checkCamera } from "./camera.ts"
import type { CameraUpdate } from "./camera.ts"
import { isFrame, writeFrame } from "./frames.ts"
import type { Frame } from "./frames.ts"
import { atlasBindings, RECORD_ATTRIBUTES } from "./glsl.ts"
import { checkParams, checkTint } from "./layer.ts"
import { materialBlank, materialSamplers, unlit } from "./material.ts"
import type { Material } from "./material.ts"
import { pickOversample } from "./oversample-math.ts"
import { checkOversample, thrashSentinel } from "./oversample.ts"
import { INSTANCE_FLOATS } from "./records.ts"
import { cellSymmetry, checkCell, checkRect, checkTileRect, chunkKey, eachChunkSlice, flagsOf, floorDiv, footprint, indexOf, isClearIndex, mod, slotOf } from "./tiles-math.ts"
import type { TileRect } from "./tiles-math.ts"

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
   * Sampler filter for the baked chunk textures at composite time; default
   * "linear", which with `oversample` is the proper resample at any scale.
   * Hard pixels belong to the ATLAS sampler (createAtlas `filter:
   * "nearest"`), not here: "nearest" on the chunks snaps texels to uneven
   * widths at a fractional scale.
   */
  filter?: FilterMode
  /**
   * What the cells look like: the layer's material (see
   * SpriteLayerOptions.material), fixed at creation; default `unlit()`.
   * Its blend is how the cells bake into their chunks ("add" for a glow
   * or light map baked over the transparent chunk clear), its params and
   * textures reach every chunk target, its instance buffers are a style
   * record per cell (`setTile`'s `style`). A tile layer BAKES, so a
   * param change re-renders every resident chunk, like the tint:
   * animated uniforms belong on a sprite layer.
   */
  material?: Material
  /** Uniform values seeded on every chunk over the material's own; see
   * TileLayer.setParams. */
  params?: ShaderParams
  /**
   * Pin the bake density: target texels per world pixel in the chunks
   * (positive integer). Absent (the default), the layer picks it from its
   * VIEWERS: the largest device pixels per world pixel any viewer reports
   * with its camera (`setCamera`'s `deviceScale` times the camera zoom),
   * rounded up, with a shrink hysteresis, bounded by `maxOversample`, by
   * `maxPixels` and the window's own pixel count per chunk, and by
   * `maxTexels` (a chunk must fit the budget). One density serves every
   * composite of the layer: a composite below it minifies the bake at
   * the leaf (level 0, linear), which aliases past about 2x under. See
   * TileLayer.setOversample.
   */
  oversample?: number
  /**
   * Cap on the auto-picked density (integer >= 1): chunks stay adaptive
   * up to it, never above. A tile world's texture memory is resident
   * chunks x n squared, so this is the lever that bounds it on
   * high-scale displays without giving up adaptivity. Ignored under a
   * pinned `oversample`.
   */
  maxOversample?: number
  /**
   * Cap on a chunk target's texel count under the auto pick, the budget
   * form of `maxOversample`: the pick stays under it on any display (the
   * window's own pixel count caps it always). Ignored under a pinned
   * `oversample`; the same name and meaning as `<SpriteLayer>`'s and the
   * 3d `<Scene>`'s.
   */
  maxPixels?: number
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
   * `setTile(cols + 1, row)` writes cell 1 and the composite repeats the
   * world where the view crosses the seam (every image of a resident
   * chunk that intersects the view). Both periods must be whole chunks
   * (`chunkTiles` divides them; the default is picked to). Not with
   * `bounds`.
   */
  wrap?: { cols: number; rows: number }
  /**
   * World pixels a frame may extend past its cell (a frame draws at its
   * own pixel size, anchored to the cell's bottom-left, so an oversized
   * tile reaches right and up): every chunk target is chunk size plus
   * this margin on every side and the composite overlaps leaves by it.
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
   * may hold at once, counted as chunk targets at the current oversample
   * (a resident chunk with no cells holds no texture but counts as one).
   * Past it the residency gate evicts the chunk least needed, the
   * furthest from any viewer in `leave`s, for one more needed; a chunk
   * nothing can make room for waits, wanted, until a view moves
   * (flux:spatial setResidencyLimit). Must hold at least one chunk
   * (throws otherwise); the default is no budget. A budget bounds the
   * resident COUNT, not the quality: oversample stays what the viewport
   * asks (`maxOversample` / `maxPixels` bound that), so a world that fills
   * the budget shows holes at its far edge rather than blur everywhere.
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
  /** The baked target while the chunk is resident and holds a cell, else
   * null. The texture is `width + 2 * overhang` wide (times the
   * oversample) with its top-left at `x - overhang, y - overhang`. */
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

/**
 * A viewer of the layer: a camera over a viewport whose chunks the
 * residency gate keeps resident (addViewer). A chunk is wanted while any
 * viewer wants it.
 */
export type TileViewer = {
  /**
   * The camera over a `width` x `height` viewport (world pixels at zoom
   * 1), pushed to the residency gate as an orthographic view at the
   * layer's flush (the frame's publish pass, a microtask outside one):
   * call it on every camera or viewport change. Absent camera fields read as the
   * defaults (projectCamera's rule). `deviceScale` is the device pixels
   * one viewport pixel covers (display scale times any ancestor fit,
   * default 1): times the camera zoom it is this viewer's wish for the
   * bake density, and the layer bakes at the largest wish (see
   * TileLayerOptions.oversample).
   */
  setCamera(camera: CameraUpdate, width: number, height: number, deviceScale?: number): void
  /** Stop viewing: chunks only this viewer wanted leave. */
  dispose(): void
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
   * before the overhang margin and the oversample. */
  readonly chunkW: number
  readonly chunkH: number
  /** The overhang margin in world pixels (see TileLayerOptions.overhang). */
  readonly overhang: number
  /** The cell bounds and the wrap periods, as given at creation. */
  readonly bounds: TileRect | null
  readonly wrap: { cols: number; rows: number } | null
  /** Target texels per world pixel, the bake density in force (pinned, or
   * the viewers' pick); see setOversample. */
  readonly oversample: number
  /**
   * Pin the bake density at `n` target texels per world pixel (positive
   * integer): every resident chunk resizes in place and re-bakes once;
   * world pixels, records and the camera are untouched, and the viewers'
   * wishes stop applying. Throws past `maxTexels` (a chunk must fit the
   * budget). `null` returns the density to the viewers' pick
   * (TileLayerOptions.oversample).
   */
  setOversample(n: number | null): void
  /**
   * Add a viewer: a camera over a viewport whose nearby chunks the gate
   * keeps resident. `<TileLayer>` adds one for its camera; imperative use
   * adds its own, since a layer with no viewer bakes nothing. Disposed
   * with the layer.
   */
  addViewer(): TileViewer
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
   * however many tiles did. A write to a non-resident chunk is records
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
   * record write - resident chunks re-render GPU-side with nothing
   * re-uploaded - but each write does re-render every resident chunk, so
   * animate it coarsely, not per frame.
   */
  setTint(tint: [number, number, number, number]): void
  /**
   * Write uniform values to every resident chunk (the material's prelude
   * uniforms, over its own `params`): LayerBase.setParams for a baked
   * layer, so each write re-renders every resident chunk, like setTint -
   * drive it from slow state, not per frame. Absent names keep their
   * values; a name the program does not declare is skipped.
   */
  setParams(params: ShaderParams): void
  dispose(): void
  /** Internal: the composites' change hook (any resident chunk's texture
   * came or went). Returns the unsubscribe. */
  _watch(fn: () => void): () => void
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
}

type ChunkStyle = { bytes: Uint8Array; buffer: BufferId | null; fields: AttributeAccess[]; floats: Float32Array | null }

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
// subscription per event, keyed by grid id.
let handlers = new Map<GridId, { enter(i: number, j: number): void; leave(i: number, j: number): void }>()
let subscribed = false

function subscribe(): void {
  if (subscribed) return
  subscribed = true
  on("spatialRegionEnter", (event: { grid?: GridId; cell?: [number, number, number] }) => {
    if (event.grid === undefined || event.cell === undefined) return
    handlers.get(event.grid)?.enter(event.cell[0], event.cell[1])
  })
  on("spatialRegionLeave", (event: { grid?: GridId; cell?: [number, number, number] }) => {
    if (event.grid === undefined || event.cell === undefined) return
    handlers.get(event.grid)?.leave(event.cell[0], event.cell[1])
  })
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

// The orthographic view a viewer pushes: world pixels to clip through the
// camera mapping (projectCamera) with the viewport's center at the view
// origin, so the gate's eye (the point enters are paced by) is the
// viewport center, and clip x/y spanning the viewport. Column-major, the
// core's matrix layout; the plane is z = 0 and the view direction -z.
function viewMatrices(camera: CameraUpdate, width: number, height: number, view: Float32Array, proj: Float32Array): void {
  let zoom = camera.zoom ?? 1
  let rotation = camera.rotation ?? 0
  let c = Math.cos(rotation)
  let s = Math.sin(rotation)
  let camX = camera.x ?? 0
  let camY = camera.y ?? 0
  view.fill(0)
  view[0] = zoom * c
  view[1] = zoom * s
  view[4] = -zoom * s
  view[5] = zoom * c
  view[10] = 1
  view[12] = (camera.pivotX ?? 0) - width / 2 - zoom * (c * camX - s * camY)
  view[13] = (camera.pivotY ?? 0) - height / 2 - zoom * (s * camX + c * camY)
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
 * textures the residency gate brings and takes as viewers move, and
 * composited as a few quads (`<TileLayer>`). Cells are unbounded; a world
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
  let oversample = opts?.oversample ?? 1
  checkOversample("createTileLayer", oversample, targetW, targetH)
  // Pinned by the option (and by setOversample), else the viewers' pick.
  let pinned = opts?.oversample !== undefined
  let maxOversample = opts?.maxOversample
  let maxPixels = opts?.maxPixels
  if (maxOversample !== undefined && !(Number.isInteger(maxOversample) && maxOversample >= 1)) {
    throw new Error(`createTileLayer: maxOversample must be a positive integer, got ${maxOversample}`)
  }
  if (maxPixels !== undefined && !(Number.isFinite(maxPixels) && maxPixels > 0)) {
    throw new Error(`createTileLayer: maxPixels must be a positive number, got ${maxPixels}`)
  }
  let thrash = thrashSentinel(`tile layer "${label}"`)
  let enter = opts?.enter ?? DEFAULT_ENTER_CHUNKS * Math.max(chunkW, chunkH)
  let leave = opts?.leave ?? Math.max(enter, DEFAULT_LEAVE_CHUNKS * Math.max(chunkW, chunkH))
  if (!(Number.isFinite(enter) && enter >= 0 && Number.isFinite(leave) && leave >= enter)) {
    throw new Error(`createTileLayer: enter must be non-negative and leave at least enter, got ${enter} and ${leave}`)
  }
  // A chunk's cost to the residency limits: its target's texels at the
  // current oversample (the grid's weight, re-pushed on setOversample).
  let chunkTexels = (n: number) => targetW * n * (targetH * n)
  let maxTexels = opts?.maxTexels
  if (maxTexels !== undefined && !(Number.isFinite(maxTexels) && maxTexels >= chunkTexels(oversample))) {
    throw new Error(`createTileLayer: maxTexels must hold at least one chunk (${chunkTexels(oversample)} texels at oversample ${oversample}), got ${maxTexels}`)
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
  let targetClass = `${TARGET_KIND}:${gpu.pipeline}`
  let bufferClass = (bytes: number) => `${BUFFER_KIND}:${bytes}`

  let disposed = false
  let scheduled = false
  // A viewer's view changed since the last flush: the spatial core is
  // flushed there, once, so the residency pass runs in the frame's
  // publish pass beside the bakes (as a sprite layer's sync flushes it),
  // never once per camera write.
  let viewsDirty = false
  // The publish pass of every frame runs the pending flush ahead of the
  // paint, so a tile set anywhere in the frame's JS is baked for that
  // frame's picture; the microtask stays for writes made outside a frame.
  // Registered outside any owner: dispose unhooks it (an autoFree: false
  // layer outlives the owner it was created in).
  let unhook = runWithOwner(null, () =>
    onBeforeRender(
      () => {
        if (scheduled) flush()
      },
      { publish: true },
    ),
  )
  // chunkKey -> chunk: every chunk the layer knows (written or resident).
  let chunks = new Map<number, Chunk>()
  let dirtyChunks: Chunk[] = []
  let watchers = new Set<() => void>()
  let viewers = new Set<TileViewer>()
  // usedRect's cache: undefined when a write invalidated it.
  let used: TileRect | null | undefined = null

  let notify = () => {
    for (let w of watchers) w()
  }
  let schedule = () => {
    if (scheduled) return
    scheduled = true
    RESOLVED.then(flush)
  }
  let flush = () => {
    scheduled = false
    if (disposed) return
    if (viewsDirty) {
      viewsDirty = false
      // The pass runs at the flush; its events arrive with the next
      // frame, which the flush requests when something changed.
      spatial.flush()
    }
    let baked = dirtyChunks
    dirtyChunks = []
    for (let chunk of baked) {
      if (!chunk.dirty) continue
      chunk.dirty = false
      // A chunk that left before its bake: nothing to bake (the cancel).
      if (chunk.texture === null || chunk.buffer === null || chunk.records === null) continue
      if (chunk.wrote) {
        chunk.wrote = false
        let out = beginBufferWrite(chunk.buffer)
        out.set(chunk.records)
        endBufferWrite(chunk.buffer, chunk.records.byteLength)
        for (let style of chunk.styles) writeBuffer(style.buffer!, style.bytes)
      }
      renderTarget(chunk.texture)
    }
  }
  let touch = (chunk: Chunk, wroteRecords = true) => {
    if (wroteRecords) chunk.wrote = true
    if (chunk.dirty) return
    chunk.dirty = true
    dirtyChunks.push(chunk)
    schedule()
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
  // Take the chunk's GPU side from the pool (or create it) and queue its
  // upload and bake; the composites learn of the texture.
  let acquire = (chunk: Chunk) => {
    if (chunk.texture !== null || chunk.records === null) return
    let bytes = chunk.records.byteLength
    // Every chunk resource carries the LAYER's label, so the inventory's
    // group under it (/gpu `groups`) is the layer's footprint.
    let buffer = (pool.take(bufferClass(bytes)) as BufferId | undefined) ?? createBuffer(bytes, { label, autoFree: false })
    chunk.buffer = buffer
    for (let style of chunk.styles) {
      style.buffer = (pool.take(bufferClass(style.bytes.byteLength)) as BufferId | undefined) ?? createBuffer(style.bytes.byteLength, { label, autoFree: false })
    }
    let buffers = [gpu.quad, buffer, ...chunk.styles.map(s => s.buffer!)]
    let w = targetW * oversample
    let h = targetH * oversample
    let texture = pool.take(targetClass) as TextureId | undefined
    if (texture !== undefined) {
      if (targetSizes.get(texture) !== w * h) setTargetSize(texture, w, h)
      setTargetParams(texture, targetParams())
      setDraw(texture, { buffers, instanceCount: perChunk })
    } else {
      texture = createShaderTarget(gpu.pipeline, w, h, targetParams(), {
        label,
        buffers,
        vertexCount: 4,
        instanceCount: perChunk,
        textures,
        clearColor: opts?.chunkClearColor ?? [0, 0, 0, 0],
        filter: opts?.filter,
        render: "manual",
        autoFree: false,
      })
    }
    targetSizes.set(texture, w * h)
    chunk.texture = texture
    touch(chunk)
    notify()
  }
  // Give the chunk's GPU side back to the pool; its records stay (the
  // caller drops them under retain "none").
  let release = (chunk: Chunk) => {
    if (chunk.texture === null) return
    pool.give(targetClass, chunk.texture)
    pool.give(bufferClass(chunk.records!.byteLength), chunk.buffer!)
    for (let style of chunk.styles) {
      pool.give(bufferClass(style.bytes.byteLength), style.buffer!)
      style.buffer = null
    }
    chunk.texture = null
    chunk.buffer = null
    chunk.dirty = false
    chunk.wrote = false
    notify()
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
    if (chunk.records === null && fill !== undefined) fill(chunk)
    if (chunk.count > 0) acquire(chunk)
    layer.onEnter?.(chunk)
  }
  let leaveChunk = (i: number, j: number) => {
    if (disposed) return
    let chunk = chunks.get(chunkKey(i, j))
    if (!chunk || !chunk.wanted) return
    chunk.wanted = false
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
    weight: chunkTexels(oversample),
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

  // Re-bake every resident chunk at density `n`: resizing a target
  // re-renders it, but a manual target's content is its last bake, so
  // each resident chunk is marked for the flush; pooled targets resize on
  // their next take. The limits weigh a chunk at its new size.
  let applyDensity = (n: number) => {
    if (n === oversample) return
    thrash()
    oversample = n
    for (let chunk of chunks.values()) {
      if (chunk.texture === null) continue
      setTargetSize(chunk.texture, targetW * n, targetH * n)
      targetSizes.set(chunk.texture, targetW * n * (targetH * n))
      touch(chunk, false)
    }
    spatial.setGridWeight(grid, chunkTexels(n))
  }
  // Each viewer's wish for the density: device pixels per world pixel.
  let wishes = new Map<TileViewer, number>()
  // The viewers' pick: the largest wish, rounded up, bounded by the caps
  // and the window's pixel count per chunk, with the shrink hysteresis
  // (oversample-math.ts); under `maxTexels` the pick is clamped so a
  // chunk always fits the budget (the fit rounds up and could not
  // otherwise). Nothing to pick from before the first viewer reports.
  let resolve = () => {
    if (pinned || wishes.size === 0) return
    let scale = 0
    for (let w of wishes.values()) scale = Math.max(scale, w)
    let win = windowSize()
    let ds = displayScale()
    let budget = win.width * ds * (win.height * ds)
    if (maxPixels !== undefined) budget = Math.min(budget, maxPixels)
    if (maxTexels !== undefined) budget = Math.min(budget, maxTexels)
    let n = pickOversample(oversample, scale, targetW, targetH, budget, limits.maxTextureSize, maxOversample)
    if (n === null) return
    if (maxTexels !== undefined) n = Math.max(1, Math.min(n, Math.floor(Math.sqrt(maxTexels / (targetW * targetH)))))
    applyDensity(n)
  }

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
    get oversample() {
      return oversample
    },
    setOversample(n) {
      if (disposed) return
      if (n === null) {
        pinned = false
        resolve()
        return
      }
      checkOversample("setOversample", n, targetW, targetH)
      if (maxTexels !== undefined && maxTexels < chunkTexels(n)) {
        throw new Error(`setOversample: maxTexels ${maxTexels} holds no chunk at oversample ${n} (${chunkTexels(n)} texels); raise it or pin lower`)
      }
      pinned = true
      applyDensity(n)
    },
    addViewer() {
      if (disposed) throw new Error("addViewer: the layer is disposed")
      let key: ViewKey | null = spatial.allocViewKey()
      let view = new Float32Array(16)
      let proj = new Float32Array(16)
      let viewer: TileViewer = {
        setCamera(camera, width, height, deviceScale = 1) {
          if (key === null || disposed) return
          checkCamera(camera)
          if (!(width > 0 && height > 0 && Number.isFinite(width) && Number.isFinite(height))) {
            throw new Error(`setCamera: the viewport must be a positive size, got ${width} x ${height}`)
          }
          if (!(Number.isFinite(deviceScale) && deviceScale > 0)) {
            throw new Error(`setCamera: deviceScale must be a positive number, got ${deviceScale}`)
          }
          viewMatrices(camera, width, height, view, proj)
          spatial.setView(key, view, proj)
          viewsDirty = true
          schedule()
          wishes.set(viewer, deviceScale * (camera.zoom ?? 1))
          resolve()
        },
        dispose() {
          if (key === null) return
          viewers.delete(viewer)
          wishes.delete(viewer)
          if (!disposed) {
            spatial.setView(key, null)
            viewsDirty = true
            schedule()
            resolve()
          }
          key = null
        },
      }
      viewers.add(viewer)
      return viewer
    },
    resident() {
      let out: TileChunk[] = []
      for (let chunk of chunks.values()) if (chunk.wanted) out.push(chunk)
      return out
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
      for (let chunk of chunks.values()) {
        if (chunk.texture === null) continue
        setTargetParams(chunk.texture, { uTint: tint })
        touch(chunk, false)
      }
    },
    setParams(next) {
      if (disposed) return
      checkParams("setParams", next)
      Object.assign(params, next)
      for (let chunk of chunks.values()) {
        if (chunk.texture === null) continue
        setTargetParams(chunk.texture, next)
        touch(chunk, false)
      }
    },
    dispose() {
      if (disposed) return
      disposed = true
      unhook()
      handlers.delete(grid)
      spatial.destroyGrid(grid)
      for (let viewer of [...viewers]) viewer.dispose()
      // The resident chunks' GPU side is destroyed outright: a shared pool
      // is not the place for a dying layer's resources.
      for (let chunk of chunks.values()) {
        if (chunk.texture === null) continue
        freeTarget(chunk.texture)
        destroyBuffer(chunk.buffer!)
        for (let style of chunk.styles) destroyBuffer(style.buffer!)
        chunk.texture = null
        chunk.buffer = null
      }
      chunks.clear()
      if (ownPool) pool.drain()
      spatial.flush()
      watchers.clear()
    },
    _watch(fn) {
      watchers.add(fn)
      return () => watchers.delete(fn)
    },
  }

  if (opts?.autoFree !== false && getOwner()) onCleanup(() => layer.dispose())
  return layer
}
