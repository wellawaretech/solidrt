// The baked tile layer: a cols x rows grid of atlas frames rendered into
// CHUNKED `render: "manual"` targets and composited as a handful of quads -
// never a quad per tile. On tiled GPUs the budget is primitive count, so a
// 100x100 world must not be 10,000 quads per frame; baked, it is a few
// chunk textures, and scrolling is a transform on the composited world
// (see <TileLayer> in components.tsx), never a repaint.
//
// Each chunk is a small copy of the sprite pipeline (shaders.ts) with fixed
// record slots - record localRow * chunkTiles + localCol IS that tile, an
// empty tile is a zero-size quad, instance count is constant per chunk.
// Records hold WORLD pixel coordinates; the chunk target's uCamera is its
// pixel origin, so the shared vertex stage does the chunk-local mapping
// (the same mechanism a camera pass uses, pointed at a chunk rect).
//
// Chunks allocate lazily on the first setTile that gives them content: an
// empty chunk costs nothing - no records, no buffer, no texture - so a
// sparse world is bounded by its content, and world size is bounded by
// memory, not maxTextureSize. setTile batches to a microtask; the flush
// publishes and re-bakes ONLY dirty chunks. A layer nobody edits publishes
// nothing, renders nothing, and costs nothing per frame. Camera-driven
// residency (bake far chunks on approach, evict them) is deliberately not
// here yet - see okf/backlog/2d-baked-layers.md.
import { getOwner, onCleanup } from "@solidrt/core"
import {
  beginBufferWrite,
  createBuffer,
  createPipelineTexture,
  destroyBuffer,
  destroyTexture,
  endBufferWrite,
  limits,
  renderTarget,
  setTargetSize,
} from "@solidrt/core/gpu"
import type { BufferId, FilterMode, TextureId } from "@solidrt/core/gpu"
import type { Frame } from "./frames.ts"
import { checkOversample } from "./oversample.ts"
import { FLOATS_PER_SPRITE } from "./records.ts"
import { FRAGMENT, INSTANCE_ATTRIBUTES, VERTEX } from "./shaders.ts"

const RESOLVED = Promise.resolve()

// Default chunk edge in pixels; the tile count per chunk derives from it.
const CHUNK_TARGET_PX = 512

export type TileLayerOptions = {
  /** Per-chunk clear color; empty (never-written) chunks render nothing. */
  clearColor?: [number, number, number, number]
  /**
   * Sampler filter for the baked chunk textures at composite time; default
   * "linear", which with `oversample` is the proper resample at any scale.
   * Hard pixels belong to the ATLAS sampler (createAtlas `filter:
   * "nearest"`), not here: "nearest" on the chunks snaps texels to uneven
   * widths at a fractional scale.
   */
  filter?: FilterMode
  /**
   * Target texels per world pixel in the baked chunks (positive integer,
   * default 1); see TileLayer.setOversample. `<TileLayer>` picks it from
   * the world view's on-screen size.
   */
  oversample?: number
  /**
   * Chunk edge in TILES; default sized so a chunk is ~512px. The tuning
   * knob between re-bake granularity (smaller = finer dirty regions) and
   * chunk count (larger = fewer textures and leaves).
   */
  chunkTiles?: number
  label?: string
  /** Skip the owner-scoped auto-dispose (see createTileLayer). */
  autoFree?: boolean
}

/** One resident chunk: a baked texture at a world-pixel rect. */
export type TileChunk = {
  texture: TextureId
  /** World-pixel origin and size of the chunk's rect. */
  x: number
  y: number
  width: number
  height: number
}

export type TileLayer = {
  /** Grid shape, fixed at creation. */
  cols: number
  rows: number
  tileW: number
  tileH: number
  /** World size in pixels: cols * tileW x rows * tileH. */
  width: number
  height: number
  /** Chunk size in world pixels (chunkTiles * tile size): the target a
   * chunk bakes into, before `oversample`. */
  chunkW: number
  chunkH: number
  /** Target texels per world pixel; see setOversample. */
  readonly oversample: number
  /**
   * Re-bake at `n` target texels per world pixel (positive integer): every
   * resident chunk resizes in place and re-bakes once; world pixels, records
   * and the camera are untouched. Pick `n` as the ceiling of the device
   * pixels one world pixel covers on screen (display scale times camera
   * zoom times any designSize fit), which `<TileLayer>` does in onLayout.
   */
  setOversample(n: number): void
  /**
   * The resident chunks, in allocation order - the layer's output.
   * Composite each as a texture leaf at its world rect (`<TileLayer>` does
   * this). The array grows as content reaches new chunks; entries never
   * move or leave (no eviction yet). Do not mutate.
   */
  chunks: TileChunk[]
  /** Called after a chunk allocates - the composition hook. Assignable. */
  onChunk?: (chunk: TileChunk) => void
  /**
   * Set one cell: a frame draws it, null clears it. Batched; the microtask
   * flush publishes and re-bakes ONLY the chunks that changed, however
   * many tiles did.
   */
  setTile(col: number, row: number, frame: Frame | null): void
  /** The frame at a cell, or null when empty. */
  getTile(col: number, row: number): Frame | null
  dispose(): void
}

type Chunk = TileChunk & {
  records: Float32Array
  buffer: BufferId
  dirty: boolean
}

/**
 * Create a baked tile layer: `cols` x `rows` cells of `tileW` x `tileH`
 * pixels, every tile drawing one atlas frame, baked into lazily-allocated
 * chunk textures and composited as a few quads. The grid shape is fixed at
 * creation - recreate the layer to resize. Disposed automatically with the
 * owning reactive scope (opt out with `{ autoFree: false }`); the atlas is
 * NOT owned - dispose it yourself.
 */
export function createTileLayer(
  cols: number,
  rows: number,
  tileW: number,
  tileH: number,
  atlas: TextureId,
  opts?: TileLayerOptions,
): TileLayer {
  if (!(cols > 0 && rows > 0 && Number.isInteger(cols) && Number.isInteger(rows))) {
    throw new Error(`createTileLayer: cols and rows must be positive integers, got ${cols} x ${rows}`)
  }
  if (!(tileW > 0 && tileH > 0)) {
    throw new Error(`createTileLayer: tile size must be positive, got ${tileW} x ${tileH}`)
  }
  let chunkTiles = opts?.chunkTiles ?? Math.max(1, Math.floor(CHUNK_TARGET_PX / Math.max(tileW, tileH)))
  if (!(chunkTiles > 0 && Number.isInteger(chunkTiles))) {
    throw new Error(`createTileLayer: chunkTiles must be a positive integer, got ${chunkTiles}`)
  }
  let chunkW = chunkTiles * tileW
  let chunkH = chunkTiles * tileH
  if (chunkW > limits.maxTextureSize || chunkH > limits.maxTextureSize) {
    throw new Error(
      `createTileLayer: chunk size ${chunkW} x ${chunkH} exceeds maxTextureSize ${limits.maxTextureSize}; lower chunkTiles`,
    )
  }
  let label = opts?.label ?? "tiles"
  let oversample = opts?.oversample ?? 1
  checkOversample("createTileLayer", oversample, chunkW, chunkH)
  let chunkCols = Math.ceil(cols / chunkTiles)
  let perChunk = chunkTiles * chunkTiles
  // One unit quad (triangle strip), shared by every chunk's pipeline.
  let quad = createBuffer(new Float32Array([-0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, 0.5]), {
    label: `${label}-quad`,
    autoFree: false,
  })

  let disposed = false
  let scheduled = false
  // Chunk index (chunkRow * chunkCols + chunkCol) -> resident chunk.
  let resident = new Map<number, Chunk>()
  let dirtyChunks: Chunk[] = []

  let flush = () => {
    scheduled = false
    if (disposed) return
    let baked = dirtyChunks
    dirtyChunks = []
    for (let chunk of baked) {
      if (!chunk.dirty) continue
      chunk.dirty = false
      let out = beginBufferWrite(chunk.buffer)
      out.set(chunk.records)
      endBufferWrite(chunk.buffer, chunk.records.byteLength)
      renderTarget(chunk.texture)
    }
  }
  let touch = (chunk: Chunk) => {
    if (chunk.dirty) return
    chunk.dirty = true
    dirtyChunks.push(chunk)
    if (scheduled) return
    scheduled = true
    RESOLVED.then(flush)
  }

  let allocate = (index: number): Chunk => {
    let x = (index % chunkCols) * chunkW
    let y = Math.floor(index / chunkCols) * chunkH
    let records = new Float32Array(perChunk * FLOATS_PER_SPRITE)
    let buffer = createBuffer(records.byteLength, { label: `${label}-chunk-records`, autoFree: false })
    let texture = createPipelineTexture(
      VERTEX,
      FRAGMENT,
      chunkW * oversample,
      chunkH * oversample,
      { uViewport: [chunkW, chunkH], uCamera: [x, y, 1, 1] },
      {
        label: `${label}-chunk`,
        topology: "triangle-strip",
        vertexCount: 4,
        attributes: [{ name: "aPos", format: "vec2" }],
        buffer: quad,
        instanceAttributes: INSTANCE_ATTRIBUTES,
        instanceBuffer: buffer,
        instanceCount: perChunk,
        blend: "alpha",
        textures: { uAtlas: atlas },
        clearColor: opts?.clearColor ?? [0, 0, 0, 0],
        filter: opts?.filter,
        render: "manual",
        autoFree: false,
      },
    )
    let chunk: Chunk = { texture, x, y, width: chunkW, height: chunkH, records, buffer, dirty: false }
    resident.set(index, chunk)
    layer.chunks.push(chunk)
    layer.onChunk?.(chunk)
    return chunk
  }

  let locate = (col: number, row: number, verb: string): [number, number] => {
    if (!(Number.isInteger(col) && Number.isInteger(row) && col >= 0 && col < cols && row >= 0 && row < rows)) {
      throw new Error(`${verb}: cell ${col}, ${row} outside the ${cols} x ${rows} grid`)
    }
    let index = Math.floor(row / chunkTiles) * chunkCols + Math.floor(col / chunkTiles)
    let at = ((row % chunkTiles) * chunkTiles + (col % chunkTiles)) * FLOATS_PER_SPRITE
    return [index, at]
  }

  let layer: TileLayer = {
    cols,
    rows,
    tileW,
    tileH,
    width: cols * tileW,
    height: rows * tileH,
    chunkW,
    chunkH,
    chunks: [],
    get oversample() {
      return oversample
    },
    setOversample(n) {
      if (disposed || n === oversample) return
      checkOversample("setOversample", n, chunkW, chunkH)
      oversample = n
      // Resizing a target re-renders it, but a manual target's content is
      // its last bake: mark every chunk so the flush bakes it at the new size.
      for (let chunk of resident.values()) {
        setTargetSize(chunk.texture, chunkW * n, chunkH * n)
        touch(chunk)
      }
    },
    setTile(col, row, frame) {
      if (disposed) return
      let [index, at] = locate(col, row, "setTile")
      let chunk = resident.get(index)
      if (frame === null) {
        // Clearing a cell no chunk holds is a no-op, not an allocation.
        if (!chunk) return
        chunk.records[at + 2] = 0
        chunk.records[at + 3] = 0
      } else {
        chunk ??= allocate(index)
        let r = chunk.records
        r[at] = (col + 0.5) * tileW
        r[at + 1] = (row + 0.5) * tileH
        r[at + 2] = tileW
        r[at + 3] = tileH
        r[at + 4] = frame.u0
        r[at + 5] = frame.v0
        r[at + 6] = frame.u1
        r[at + 7] = frame.v1
        r[at + 9] = 1
        r[at + 10] = 1
        r[at + 11] = 1
        r[at + 12] = 1
      }
      touch(chunk)
    },
    getTile(col, row) {
      let [index, at] = locate(col, row, "getTile")
      let chunk = resident.get(index)
      if (!chunk || chunk.records[at + 2] === 0) return null
      let r = chunk.records
      return { u0: r[at + 4]!, v0: r[at + 5]!, u1: r[at + 6]!, v1: r[at + 7]! }
    },
    dispose() {
      if (disposed) return
      disposed = true
      for (let chunk of resident.values()) {
        destroyTexture(chunk.texture)
        destroyBuffer(chunk.buffer)
      }
      resident.clear()
      layer.chunks.length = 0
      destroyBuffer(quad)
    },
  }

  if (opts?.autoFree !== false && getOwner()) onCleanup(() => layer.dispose())
  return layer
}
