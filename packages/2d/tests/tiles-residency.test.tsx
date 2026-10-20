// The tile layer as a handler over the residency gate: chunks under a
// view enter and bake, chunks it moves away from leave and hand
// their target and buffers to the pool, which the next enter takes (the
// same texture ids after a round trip), `retain: "none"` drops records
// and regenerates through `fill` on every enter, the eight cell symmetries
// bake as Tiled composes them, an oversized frame bakes its overhang into
// the OWNING chunk and a view draws the lower row over the row above, a
// wrapped world's seam is two entries over one texture, and the
// unbounded addressing (negative cells, a cell 10^6 cells out, bounds,
// usedRect) holds. The layer is GPU state, so these are app tests on the
// dev client; the pixel reads go through readTexture on the chunk target
// and on the view target, and through the leaf's painted pixels.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import type { DecodedImage } from "@solidrt/core"
import { createSignal } from "@solidrt/core"
import { readTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { createAtlas, createTileLayer, fullFrame, setBakeBudget, TileLayer, TILE_FLIP_X, TILE_FLIP_Y, TILE_TRANSPOSE } from "../src/index.ts"
import type { TileCamera, TileChunk, TileLayerHandle, TileLayerOptions, ViewHandle } from "../src/index.ts"

// Cells are 8 px, chunks two cells (16 px): a 32 x 32 viewport at the
// origin sits exactly over the four chunks of the first quadrant.
const TILE = 8
const CHUNK_TILES = 2
const CHUNK = TILE * CHUNK_TILES
const VIEW = 32
// The residency band: enter 0 (the strict test, so a chunk touching the
// view's edge is not wanted), leave one chunk.
const BAND = { enter: 0, leave: CHUNK }
// The seeded world: cells -16..48 both ways, so every chunk a test's
// camera reaches holds cells.
const SEED = { col: -16, row: -16, cols: 64, rows: 64 }
const WHITE = [255, 255, 255, 255]
const RED = [255, 0, 0, 255]
const GREEN = [0, 255, 0, 255]
const BLUE = [0, 0, 255, 255]
const TRANSPARENT = [0, 0, 0, 0]

/** A solid w x h image. */
function solid(rgb: [number, number, number], w: number, h: number): DecodedImage {
  let data = new Uint8Array(w * h * 4)
  for (let i = 0; i < w * h; i++) data.set([rgb[0], rgb[1], rgb[2], 255], i * 4)
  return { width: w, height: h, data }
}

/** A layer of white cells over SEED, built inside the mounted app. */
async function mounted(app: TestApp, opts: TileLayerOptions = {}, seed = opts.retain !== "none"): Promise<{ layer: TileLayerHandle; white: TextureId }> {
  let built!: { layer: TileLayerHandle; white: TextureId }
  await app.mount(() => {
    let atlas = createAtlas(solid([255, 255, 255], TILE, TILE), { label: "res-atlas" })
    let layer = createTileLayer(TILE, TILE, [atlas], { chunkTiles: CHUNK_TILES, frames: [fullFrame(atlas)], label: "res", ...BAND, ...opts })
    if (seed) layer.setTiles(SEED.col, SEED.row, SEED.cols, SEED.rows, new Uint16Array(SEED.cols * SEED.rows).fill(0))
    built = { layer, white: atlas.texture }
    return <view width={VIEW} height={VIEW} />
  })
  return built
}

function pixel(texture: TextureId, x: number, y: number): number[] {
  let { width, data } = readTexture(texture)
  let at = (y * width + x) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!]
}

let cellsOf = (chunks: TileChunk[]) => chunks.map(c => `${c.col},${c.row}`).sort()
/** The draw entries of the view target labelled `label`. */
function entriesOf(app: TestApp, label: string): unknown[] {
  let targets = app.gpu({ label }).pipelines as { kind: string; draws?: unknown[] }[]
  let view = targets.find(t => t.kind === "draws")
  return view?.draws ?? []
}
let texturesOf = (chunks: TileChunk[]) => chunks.map(c => c.texture).sort()

test("chunks under the view enter and bake; the ones the camera leaves behind leave, and the pool hands their targets to the ones entering", async app => {
  let { layer } = await mounted(app)
  let entered: string[] = []
  let left: string[] = []
  layer.onEnter = c => entered.push(`${c.col},${c.row}`)
  layer.onLeave = c => left.push(`${c.col},${c.row}`)
  let view = layer.createView({ width: VIEW, height: VIEW })
  await app.settle()
  expect(cellsOf(layer.resident())).toEqual(["0,0", "0,2", "2,0", "2,2"])
  expect(entered.sort()).toEqual(["0,0", "0,2", "2,0", "2,2"])
  let first = layer.resident()
  for (let c of first) expect(c.texture === null).toBe(false)
  let before = texturesOf(first)
  expect((app.gpu({ label: "res" }).textures as unknown[]).length).toBe(4)
  // The bake landed: the chunk's texture is white where its cells are.
  expect(pixel(first[0]!.texture!, TILE / 2, TILE / 2)).toEqual(WHITE)
  // Two chunks to the right: the old four are a chunk past leave.
  view.setCamera({ x: 2 * VIEW })
  await app.settle()
  expect(left.sort()).toEqual(["0,0", "0,2", "2,0", "2,2"])
  expect(cellsOf(layer.resident())).toEqual(["10,0", "10,2", "8,0", "8,2"])
  expect(texturesOf(layer.resident())).toEqual(before)
  expect((app.gpu({ label: "res" }).textures as unknown[]).length).toBe(4)
  // Half a chunk back: the chunk now under the view's left edge enters,
  // the ones a chunk away stay out (inside the leave band or not, a
  // chunk that left is not re-entered until it is within enter again).
  view.setCamera({ x: 2 * VIEW - CHUNK / 2 })
  await app.settle()
  expect(cellsOf(layer.resident())).toEqual(["10,0", "10,2", "6,0", "6,2", "8,0", "8,2"])
  expect(left.length).toBe(4)
  // A quarter chunk further back: chunk 4 is a chunk past enter still.
  view.setCamera({ x: 2 * VIEW - CHUNK / 2 - CHUNK / 4 })
  await app.settle()
  expect(cellsOf(layer.resident())).toEqual(["10,0", "10,2", "6,0", "6,2", "8,0", "8,2"])
  // The view goes: everything leaves.
  view.dispose()
  await app.settle()
  expect(layer.resident()).toEqual([])
  expect(left.length).toBe(10)
})

test("a chunk written while resident re-bakes; an empty resident chunk holds no texture until a cell arrives", async app => {
  let { layer } = await mounted(app, {}, false)
  let view = layer.createView({ width: VIEW, height: VIEW })
  await app.settle()
  // Nothing seeded: resident and empty, so no targets.
  expect(layer.resident().length).toBe(4)
  for (let c of layer.resident()) expect(c.texture).toBe(null)
  expect((app.gpu({ label: "res" }).textures as unknown[]).length).toBe(0)
  let white = fullFrame(layer.atlases[0]!)
  layer.setTile(0, 0, white)
  await app.settle()
  let chunk = layer.resident().find(c => c.col === 0 && c.row === 0)!
  expect(chunk.texture === null).toBe(false)
  expect(pixel(chunk.texture!, TILE / 2, TILE / 2)).toEqual(WHITE)
  expect(pixel(chunk.texture!, TILE + TILE / 2, TILE / 2)).toEqual(TRANSPARENT)
  layer.setTile(1, 0, white)
  await app.settle()
  expect(pixel(chunk.texture!, TILE + TILE / 2, TILE / 2)).toEqual(WHITE)
  // Cleared empty, the chunk gives its texture back.
  layer.setTile(0, 0, null)
  layer.setTile(1, 0, null)
  await app.settle()
  expect(chunk.texture).toBe(null)
})

test("retain none: fill generates a chunk's cells on every enter and a leave drops them", async app => {
  let fills: string[] = []
  let { layer } = await mounted(app, {
    retain: "none",
    fill: chunk => {
      fills.push(`${chunk.col},${chunk.row}`)
      layer.setTiles(chunk.col, chunk.row, chunk.cols, chunk.rows, new Uint16Array(chunk.cols * chunk.rows).fill(0))
    },
  })
  let view = layer.createView({ width: VIEW, height: VIEW })
  await app.settle()
  expect(fills.sort()).toEqual(["0,0", "0,2", "2,0", "2,2"])
  expect(layer.getTile(0, 0) === null).toBe(false)
  expect(layer.usedRect()).toEqual({ col: 0, row: 0, cols: 4, rows: 4 })
  for (let c of layer.resident()) expect(c.texture === null).toBe(false)
  view.setCamera({ x: 2 * VIEW })
  await app.settle()
  expect(layer.getTile(0, 0)).toBe(null)
  expect(layer.usedRect()).toEqual({ col: 8, row: 0, cols: 4, rows: 4 })
  view.setCamera({ x: 0 })
  await app.settle()
  expect(fills.length).toBe(12)
  expect(layer.getTile(0, 0) === null).toBe(false)
})

// A 2 x 2 texel atlas: red, green over blue, white.
const QUAD: DecodedImage = { width: 2, height: 2, data: new Uint8Array([255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]) }
const QUAD_COLORS = [RED, GREEN, BLUE, WHITE]

// Tiled's rule over a texel's offset from the frame's center: the
// diagonal flip first, then the horizontal, then the vertical.
function placed(flipX: boolean, flipY: boolean, transpose: boolean, sx: number, sy: number): [number, number] {
  let x = sx - 0.5
  let y = sy - 0.5
  if (transpose) [x, y] = [y, x]
  if (flipX) x = -x
  if (flipY) y = -y
  return [x + 0.5, y + 0.5]
}

test("the eight cell symmetries bake as Tiled composes them, through setTile's flags and setTiles' index bits alike", async app => {
  let layer!: TileLayerHandle
  await app.mount(() => {
    let atlas = createAtlas(QUAD, { filter: "nearest", label: "sym-atlas" })
    layer = createTileLayer(2, 2, [atlas], { chunkTiles: 8, frames: [fullFrame(atlas)], label: "sym", ...BAND })
    let frame = fullFrame(atlas)
    // Row 0 through setTile's flags, row 1 through the index bits.
    let indices = new Uint32Array(8)
    for (let bits = 0; bits < 8; bits++) {
      let flipX = (bits & 4) !== 0
      let flipY = (bits & 2) !== 0
      let transpose = (bits & 1) !== 0
      layer.setTile(bits, 0, frame, { flipX, flipY, transpose })
      indices[bits] = ((flipX ? TILE_FLIP_X : 0) | (flipY ? TILE_FLIP_Y : 0) | (transpose ? TILE_TRANSPOSE : 0)) >>> 0
    }
    layer.setTiles(0, 1, 8, 1, indices)
    return <view width={16} height={16} />
  })
  layer.createView({ width: 16, height: 16 })
  await app.settle()
  let chunk = layer.resident()[0]!
  expect(chunk.texture === null).toBe(false)
  for (let bits = 0; bits < 8; bits++) {
    let flipX = (bits & 4) !== 0
    let flipY = (bits & 2) !== 0
    let transpose = (bits & 1) !== 0
    for (let sy = 0; sy < 2; sy++) {
      for (let sx = 0; sx < 2; sx++) {
        let [dx, dy] = placed(flipX, flipY, transpose, sx, sy)
        for (let row = 0; row < 2; row++) {
          let got = pixel(chunk.texture!, bits * 2 + dx, row * 2 + dy)
          let want = QUAD_COLORS[sy * 2 + sx]!
          if (got.join() !== want.join()) throw new Error(`flags x${flipX} y${flipY} d${transpose} row ${row}: texel ${sx},${sy} at ${dx},${dy} is [${got}], want [${want}]`)
        }
      }
    }
  }
  // getTile reads the canonical frame back whatever the symmetry.
  expect(layer.getTile(7, 1)).toEqual(fullFrame(layer.atlases[0]!))
})

test("an oversized frame bakes its overhang into the owning chunk, and the view draws the lower row over the row above", async app => {
  let layer!: TileLayerHandle
  let view!: ViewHandle
  let win = await app.mount(() => {
    // A tall frame: red over green, two cells high.
    let tall = createAtlas({ width: TILE, height: 2 * TILE, data: new Uint8Array([...solid([255, 0, 0], TILE, TILE).data, ...solid([0, 255, 0], TILE, TILE).data]) }, { label: "tall-atlas" })
    let blue = createAtlas(solid([0, 0, 255], TILE, TILE), { label: "blue-atlas" })
    return (
      <TileLayer
        ref={l => (layer = l)}
        viewRef={v => (view = v)}
        tileW={TILE}
        tileH={TILE}
        atlases={[tall, blue]}
        frames={[fullFrame(tall), fullFrame(blue)]}
        chunkTiles={CHUNK_TILES}
        enter={BAND.enter}
        leave={BAND.leave}
        width={VIEW}
        height={VIEW}
        camera={{}}
        label="over"
      />
    )
  })
  // The table's tall frame sets the margin to one cell.
  expect(layer.overhang).toBe(TILE)
  let tall = layer.frames![0]!
  let blue = layer.frames![1]!
  // The upper chunk's bottom row cell (0, 1) is blue; the lower chunk's
  // top row cell (0, 2) carries the tall frame, whose red half overhangs
  // into cell (0, 1)'s area.
  layer.setTile(0, 1, blue)
  layer.setTile(0, 2, tall)
  await app.settle()
  let lower = layer.resident().find(c => c.col === 0 && c.row === 2)!
  let upper = layer.resident().find(c => c.col === 0 && c.row === 0)!
  // The lower chunk's texture starts one margin above and left of its
  // rect: at column 8..15, rows 0..7 are the overhang (red), rows 8..15
  // the cell itself (green).
  expect(pixel(lower.texture!, TILE + TILE / 2, TILE / 2)).toEqual(RED)
  expect(pixel(lower.texture!, TILE + TILE / 2, TILE + TILE / 2)).toEqual(GREEN)
  // The upper chunk bakes only its own cell: blue where cell (0, 1) is,
  // nothing of the overhang.
  expect(pixel(upper.texture!, TILE + TILE / 2, TILE + TILE + TILE / 2)).toEqual(BLUE)
  // Through the view, the red overhang (a lower row) covers the blue
  // cell: one leaf, the view's target, two chunk entries in row order.
  expect(win.findAll({ kind: "texture" }).length).toBe(1)
  expect(entriesOf(app, "over").length).toBe(2)
  expect(pixel(view.texture, TILE / 2, TILE + TILE / 2)).toEqual(RED)
  expect(pixel(view.texture, TILE / 2, 2 * TILE + TILE / 2)).toEqual(GREEN)
  expect(win.pixel(TILE / 2, TILE + TILE / 2)).toEqual(RED)
})

test("a wrapped world repeats across the seam: every image of a resident chunk inside the view is an entry over the one texture", async app => {
  let layer!: TileLayerHandle
  let view!: ViewHandle
  let [camera, setCamera] = createSignal<TileCamera>({})
  let win = await app.mount(() => {
    let white = createAtlas(solid([255, 255, 255], TILE, TILE), { label: "wrap-white" })
    let red = createAtlas(solid([255, 0, 0], TILE, TILE), { label: "wrap-red" })
    return <TileLayer ref={l => (layer = l)} viewRef={v => (view = v)} tileW={TILE} tileH={TILE} atlases={[white, red]} wrap={{ cols: 2, rows: 2 }} enter={BAND.enter} leave={BAND.leave} width={40} height={TILE * 2} camera={camera()} label="wrap" />
  })
  let [white, red] = layer.atlases
  // One chunk is the whole world; the default chunkTiles divides the periods.
  expect(layer.chunkTiles).toBe(2)
  layer.setTiles(0, 0, 2, 2, [fullFrame(white!), fullFrame(white!), fullFrame(white!), fullFrame(white!)])
  // Modular cells: (3, 1) is (1, 1).
  layer.setTile(3, 1, fullFrame(red!))
  expect(layer.getTile(1, 1)!.texture).toBe(red!.texture)
  await app.settle()
  expect(layer.resident().length).toBe(1)
  // The world is 16 px wide under a 40 px view: images at 0, 16 and 32,
  // three entries of the view over the one chunk.
  expect(win.findAll({ kind: "texture" }).length).toBe(1)
  expect(entriesOf(app, "wrap").length).toBe(3)
  expect(pixel(view.texture, TILE / 2, TILE / 2)).toEqual(WHITE)
  expect(pixel(view.texture, 2 * TILE + TILE + TILE / 2, TILE + TILE / 2)).toEqual(RED)
  expect(pixel(view.texture, 4 * TILE + TILE / 2, TILE / 2)).toEqual(WHITE)
  expect(win.pixel(2 * TILE + TILE + TILE / 2, TILE + TILE / 2)).toEqual(RED)
  // The camera moves half a world: still three images, the seam elsewhere.
  setCamera({ x: TILE })
  await app.settle()
  expect(entriesOf(app, "wrap").length).toBe(3)
  expect(pixel(view.texture, TILE / 2, TILE + TILE / 2)).toEqual(RED)
  // A camera a thousand worlds away: the same three entries, re-keyed.
  setCamera({ x: 1000 * 2 * TILE + TILE, y: -700 * 2 * TILE })
  await app.settle()
  expect(entriesOf(app, "wrap").length).toBe(3)
  expect(pixel(view.texture, TILE / 2, TILE + TILE / 2)).toEqual(RED)
})

// A checker world: 8 px cells alternating white and black over 40 x 40
// cells in four chunks, so a view pixel covering 16 world px (zoom 1/16)
// averages a 2 x 2 block of cells - mid-gray - when the bake is minified
// through its mip chain, and lands on one cell's texel at level 0. The
// camera sits half a cell up and left, so every view pixel's center falls
// on a cell center (16i + 4), never on a cell edge where level 0 would
// average the four cells meeting there.
const CHECKER_CELLS = 40
const CHECKER_CHUNK_TILES = 20
const MINIFY_ZOOM = 1 / 16
const MINIFIED_VIEW = (CHECKER_CELLS * TILE) / 16
/** Mid-gray within a trilinear tap's rounding. */
const GRAY_TOLERANCE = 24

/** Two checker layers in one app, one with the chain and one without,
 * each with a minified view. */
async function checkerViews(app: TestApp): Promise<{ chained: ViewHandle; flat: ViewHandle }> {
  let views!: { chained: ViewHandle; flat: ViewHandle }
  await app.mount(() => {
    let white = createAtlas(solid([255, 255, 255], TILE, TILE), { label: "checker-white" })
    let black = createAtlas(solid([0, 0, 0], TILE, TILE), { label: "checker-black" })
    let cells = new Uint16Array(CHECKER_CELLS * CHECKER_CELLS)
    for (let row = 0; row < CHECKER_CELLS; row++) for (let col = 0; col < CHECKER_CELLS; col++) cells[row * CHECKER_CELLS + col] = (col + row) & 1
    let make = (mipmap: boolean, label: string) => {
      let layer = createTileLayer(TILE, TILE, [white, black], { chunkTiles: CHECKER_CHUNK_TILES, frames: [fullFrame(white), fullFrame(black)], mipmap, label, ...BAND })
      layer.setTiles(0, 0, CHECKER_CELLS, CHECKER_CELLS, cells)
      return layer.createView({ width: MINIFIED_VIEW, height: MINIFIED_VIEW, camera: { x: -TILE / 2, y: -TILE / 2, zoom: MINIFY_ZOOM }, label: `${label}-view` })
    }
    views = { chained: make(true, "checker"), flat: make(false, "checker-flat") }
    return <view width={VIEW} height={VIEW} />
  })
  await app.settle()
  return views
}

test("a view far below the bake density minifies through the chunks' mip chain: a checker averages to gray, and without the chain it aliases to whole cells", async app => {
  let { chained, flat } = await checkerViews(app)
  expect(entriesOf(app, "checker-view").length).toBe(4)
  let img = readTexture(chained.texture)
  expect(img.width).toBe(MINIFIED_VIEW)
  for (let i = 0; i < img.data.length; i += 4) {
    let v = img.data[i]!
    if (Math.abs(v - 128) > GRAY_TOLERANCE) throw new Error(`minified pixel ${i / 4} is ${v}, not mid-gray`)
    expect(img.data[i + 3]).toBe(255)
  }
  let aliased = readTexture(flat.texture)
  for (let i = 0; i < aliased.data.length; i += 4) {
    let v = aliased.data[i]!
    if (v !== 0 && v !== 255) throw new Error(`level-0 pixel ${i / 4} is ${v}, not a whole cell`)
  }
})

test("unbounded addressing: negative and far cells, bounds as a clamp, usedRect as the written extent, the creation checks", async app => {
  let open!: TileLayerHandle
  let clamped!: TileLayerHandle
  let frame!: ReturnType<typeof fullFrame>
  let tall!: ReturnType<typeof fullFrame>
  await app.mount(() => {
    let atlas = createAtlas(solid([255, 255, 255], TILE, TILE), { label: "addr-atlas" })
    let tallAtlas = createAtlas(solid([255, 255, 255], TILE, 2 * TILE), { label: "addr-tall" })
    frame = fullFrame(atlas)
    tall = fullFrame(tallAtlas)
    open = createTileLayer(TILE, TILE, [atlas, tallAtlas], { chunkTiles: CHUNK_TILES, label: "open" })
    clamped = createTileLayer(TILE, TILE, [atlas], { chunkTiles: CHUNK_TILES, bounds: { col: -4, row: -4, cols: 8, rows: 8 }, label: "clamped" })
    expect(() => createTileLayer(TILE, TILE, [atlas], { bounds: { col: 0, row: 0, cols: 4, rows: 4 }, wrap: { cols: 4, rows: 4 } })).toThrow()
    expect(() => createTileLayer(TILE, TILE, [atlas], { wrap: { cols: 6, rows: 4 }, chunkTiles: 4 })).toThrow()
    expect(() => createTileLayer(TILE, TILE, [atlas, tallAtlas], { frames: [fullFrame(tallAtlas)], overhang: 4 })).toThrow()
    return <view width={VIEW} height={VIEW} />
  })
  expect(open.usedRect()).toBe(null)
  open.setTile(-1, -1, frame)
  open.setTile(10 ** 6, -(10 ** 6), frame)
  expect(open.getTile(-1, -1)).toEqual(frame)
  expect(open.getTile(10 ** 6, -(10 ** 6))).toEqual(frame)
  expect(open.usedRect()).toEqual({ col: -1, row: -(10 ** 6), cols: 10 ** 6 + 2, rows: 10 ** 6 })
  open.setTile(10 ** 6, -(10 ** 6), null)
  expect(open.usedRect()).toEqual({ col: -1, row: -1, cols: 1, rows: 1 })
  // The far cell's chunk bakes at its own rect, chunk-local records.
  open.setTile(10 ** 6, 10 ** 6, frame)
  open.createView({ width: VIEW, height: VIEW, camera: { x: 10 ** 6 * TILE, y: 10 ** 6 * TILE } })
  await app.settle()
  let far = open.resident().find(c => c.texture !== null)!
  expect(far.col).toBe(10 ** 6)
  expect(far.x).toBe(10 ** 6 * TILE)
  expect(pixel(far.texture!, TILE / 2, TILE / 2)).toEqual(WHITE)
  expect(pixel(far.texture!, TILE + TILE / 2, TILE / 2)).toEqual(TRANSPARENT)
  // An oversized frame throws without a margin, transposed too.
  expect(() => open.setTile(0, 0, tall)).toThrow()
  expect(() => open.setTile(0, 0, tall, { transpose: true })).toThrow()
  expect(open.getTile(0, 0)).toBe(null)
  // The clamp.
  clamped.setTile(-4, -4, frame)
  clamped.setTile(3, 3, frame)
  expect(clamped.usedRect()).toEqual({ col: -4, row: -4, cols: 8, rows: 8 })
  expect(() => clamped.setTile(4, 0, frame)).toThrow()
  expect(() => clamped.setTile(0, -5, frame)).toThrow()
  expect(() => clamped.setTiles(2, 2, 3, 1, [frame, frame, frame])).toThrow()
  expect(() => clamped.getTile(4, 4)).toThrow()
  expect(clamped.bounds).toEqual({ col: -4, row: -4, cols: 8, rows: 8 })
})

test("maxTexels caps the resident chunks: the gate evicts the chunk furthest from the view for a nearer one, leave before enter, and the inventory groups the layer under its label", async app => {
  // One chunk's texels at density 1 (16 x 16, no overhang, no chain): a
  // budget of exactly one chunk.
  let { layer } = await mounted(app, { maxTexels: CHUNK * CHUNK, mipmap: false })
  let log: string[] = []
  layer.onEnter = c => log.push(`enter ${c.col},${c.row}`)
  layer.onLeave = c => log.push(`leave ${c.col},${c.row}`)
  // A 48 x 16 viewport over the chunks at cell columns 0, 2 and 4, the
  // eye over the middle one: it alone is resident, the other two wait on
  // the budget.
  let view = layer.createView({ width: 3 * CHUNK, height: CHUNK })
  await app.settle()
  expect(cellsOf(layer.resident())).toEqual(["2,0"])
  expect(log).toEqual(["enter 2,0"])
  let first = layer.resident()[0]!.texture
  let group = (app.gpu({ label: "res" }).groups as Record<string, { textures: number; buffers: number; bytes: number }>)["res"]!
  expect(group.textures).toBe(1)
  expect(group.buffers).toBe(1)
  expect(group.bytes > 0).toBe(true)
  // The gate as /stats reports it: one resident, two wanted but stalled
  // behind the budget, the resident weight one chunk's bytes (rgba8
  // texels: the gate weighs in bytes, the layer's budget is in texels).
  let stats = app.stats() as { regionsResident: number; regionsStalled: number; regionsWanted: number; residentWeight: number; app: { poolMisses: number; poolHits: number } }
  expect([stats.regionsResident, stats.regionsStalled, stats.regionsWanted]).toEqual([1, 2, 3])
  expect(stats.residentWeight).toBe(CHUNK * CHUNK * 4)
  let misses = stats.app.poolMisses
  // At rest the GPU is done with every frame submitted (settle waits for
  // the raster thread), which is what lets the pool hand a freed set
  // straight over below: it hands out only what no frame in flight reads.
  let counts = app.stats() as { submittedFrames: number; retiredFrames: number }
  expect(counts.retiredFrames).toBe(counts.submittedFrames)
  // Pan a chunk right: the chunk at column 4 is now under the eye and more
  // needed than the one at column 2, which the budget evicts first, so its
  // target is handed over.
  view.setCamera({ x: CHUNK })
  await app.settle()
  expect(log).toEqual(["enter 2,0", "leave 2,0", "enter 4,0"])
  expect(cellsOf(layer.resident())).toEqual(["4,0"])
  expect(layer.resident()[0]!.texture).toBe(first)
  expect((app.gpu({ label: "res" }).textures as unknown[]).length).toBe(1)
  // The handover was two pool hits (the target and the records buffer),
  // no create.
  let after = app.stats() as { app: { poolMisses: number; poolHits: number } }
  expect(after.app.poolMisses).toBe(misses)
  expect(after.app.poolHits >= 2).toBe(true)
})

test("maxTexels must hold a chunk, at creation and at a new density; a chunk's mip chain weighs a third more", async app => {
  await app.mount(() => {
    let atlas = createAtlas(solid([255, 255, 255], TILE, TILE), { label: "res-atlas" })
    expect(() => createTileLayer(TILE, TILE, [atlas], { chunkTiles: CHUNK_TILES, maxTexels: CHUNK * CHUNK - 1, mipmap: false })).toThrow(/at least one chunk/)
    // The default chain: level 0 alone no longer holds a chunk.
    expect(() => createTileLayer(TILE, TILE, [atlas], { chunkTiles: CHUNK_TILES, maxTexels: CHUNK * CHUNK })).toThrow(/at least one chunk/)
    let layer = createTileLayer(TILE, TILE, [atlas], { chunkTiles: CHUNK_TILES, maxTexels: CHUNK * CHUNK, mipmap: false })
    expect(() => layer.setDensity(2)).toThrow(/holds no chunk/)
    let chained = createTileLayer(TILE, TILE, [atlas], { chunkTiles: CHUNK_TILES, maxTexels: Math.ceil((CHUNK * CHUNK * 4) / 3) })
    expect(chained.mipmap).toBe(true)
    return <view width={VIEW} height={VIEW} />
  })
})

test("the layer bakes at the largest density its views ask for, shrinks when that view goes, and a budget clamps the pick instead of throwing", async app => {
  let { layer } = await mounted(app)
  let main = layer.createView({ width: VIEW, height: VIEW, label: "main" })
  let mini = layer.createView({ width: VIEW, height: VIEW, camera: { zoom: 0.25 }, label: "mini" })
  // A main view at oversample 1 and a minimap zoomed out: density 1.
  await app.settle()
  expect(layer.density).toBe(1)
  // The main view goes to oversample 3 (a 2.3x display rounded up by its
  // auto pick): the bake follows it.
  main.setOversample(3)
  await app.settle()
  expect(layer.density).toBe(3)
  let chunk = layer.resident().find(c => c.texture !== null)!
  expect(readTexture(chunk.texture!).width).toBe(CHUNK * 3)
  // The main view goes: the minimap's wish (0.25) is what is left.
  main.dispose()
  await app.settle()
  expect(layer.density).toBe(1)
  // A pin overrides the views (the minimap at oversample 4 wishes 1);
  // null hands the pick back, and a wish of 0.25 is far enough under 2
  // for the shrink hysteresis to let go.
  layer.setDensity(2)
  mini.setOversample(4)
  await app.settle()
  expect(layer.density).toBe(2)
  mini.setOversample(1)
  layer.setDensity(null)
  await app.settle()
  expect(layer.density).toBe(1)
  // Oversample 8 at zoom 0.25: a wish of 2.
  mini.setOversample(8)
  await app.settle()
  expect(layer.density).toBe(2)
})

test("under maxTexels the views' pick is clamped to what the budget holds; only a pin past it throws", async app => {
  // Two chunks' worth at density 1: one chunk at density 1 only.
  let { layer } = await mounted(app, { maxTexels: 2 * CHUNK * CHUNK, mipmap: false })
  layer.createView({ width: VIEW, height: VIEW, oversample: 3 })
  await app.settle()
  expect(layer.density).toBe(1)
  expect(() => layer.setDensity(2)).toThrow(/holds no chunk/)
})

test("<TileLayer layer> shows an existing layer again under a camera of its own: one more view with entries of its own, the maker's props and hooks refused", async app => {
  let layer!: TileLayerHandle
  let [camera, setCamera] = createSignal<TileCamera>({ x: 2 * VIEW })
  let win = await app.mount(() => {
    let atlas = createAtlas(solid([255, 255, 255], TILE, TILE), { label: "adopt-atlas" })
    layer = createTileLayer(TILE, TILE, [atlas], { chunkTiles: CHUNK_TILES, frames: [fullFrame(atlas)], label: "adopt", ...BAND })
    layer.setTiles(SEED.col, SEED.row, SEED.cols, SEED.rows, new Uint16Array(SEED.cols * SEED.rows).fill(0))
    return (
      <view flexDirection="row">
        <TileLayer layer={layer} width={VIEW} height={VIEW} camera={{}} label="main" />
        <TileLayer layer={layer} width={VIEW} height={VIEW} camera={camera()} label="mini" />
      </view>
    )
  })
  await app.settle()
  // Resident where either view looks: the first quadrant under the
  // main camera, two viewports to the right under the minimap's.
  expect(cellsOf(layer.resident())).toEqual(["0,0", "0,2", "2,0", "2,2", "8,0", "8,2", "10,0", "10,2"].sort())
  // Each view draws every resident chunk (the target's edge clips).
  expect(entriesOf(app, "main").length).toBe(8)
  expect(entriesOf(app, "mini").length).toBe(8)
  let [main, mini] = win.findAll({ kind: "texture" })
  expect(main!.pixel(TILE / 2, TILE / 2)).toEqual(WHITE)
  expect(mini!.pixel(TILE / 2, TILE / 2)).toEqual(WHITE)
  // The minimap's camera moves on: its chunks leave, the main view's stay.
  setCamera({ x: 20 * VIEW })
  await app.settle()
  expect(cellsOf(layer.resident())).toEqual(["0,0", "0,2", "2,0", "2,2", "80,0", "80,2", "82,0", "82,2"].sort())
  // The maker's props and hooks are refused with `layer`.
  expect(() => TileLayer({ layer, tileW: TILE } as never)).toThrow(/fixed when it was created/)
  expect(() => TileLayer({ layer, onEnter: () => {} } as never)).toThrow(/hooks/)
})

// The bake budget test's camera: a view of 32 px with its top-left at
// minus half a chunk, so its eye (the viewport center) sits on the center
// of chunk (0, 0) and its box [-8, 24) covers nine chunks.
const PACE_CAMERA = { x: -CHUNK / 2, y: -CHUNK / 2 }
const EYE = CHUNK / 2
// The cell column the test's red cells start at: the chunks a pan to the
// right brings in, told from a pooled target's white by their own color.
const RED_FROM_COL = 4
// The engine's default bake budget, restored after the test (the budget
// is app-wide).
const DEFAULT_BAKE_BUDGET = 1

test("bakes are paced under the bake budget: re-bakes of drawn chunks go nearest the view's eye first while the rest show their last bake, and a chunk entering bakes in its own frame and spends the budget", async app => {
  let layer!: TileLayerHandle
  let [camera, setCamera] = createSignal<TileCamera>(PACE_CAMERA)
  let white!: ReturnType<typeof fullFrame>
  let red!: ReturnType<typeof fullFrame>
  await app.mount(() => {
    let whiteAtlas = createAtlas(solid([255, 255, 255], TILE, TILE), { label: "pace-white" })
    let redAtlas = createAtlas(solid([255, 0, 0], TILE, TILE), { label: "pace-red" })
    white = fullFrame(whiteAtlas)
    red = fullFrame(redAtlas)
    return <TileLayer ref={l => (layer = l)} tileW={TILE} tileH={TILE} atlases={[whiteAtlas, redAtlas]} frames={[white, red]} chunkTiles={CHUNK_TILES} enter={BAND.enter} leave={BAND.leave} width={VIEW} height={VIEW} camera={camera()} label="pace" />
  })
  // White cells over the seed, red from RED_FROM_COL on.
  layer.setTiles(SEED.col, SEED.row, SEED.cols, SEED.rows, new Uint16Array(SEED.cols * SEED.rows).fill(0))
  let redCols = SEED.col + SEED.cols - RED_FROM_COL
  layer.setTiles(RED_FROM_COL, SEED.row, redCols, SEED.rows, new Uint16Array(redCols * SEED.rows).fill(1))
  await app.settle()
  // The chunk's first cell, the one the test recolors.
  let at = (c: TileChunk) => pixel(c.texture!, TILE / 2, TILE / 2)
  let reds = () => layer.resident().filter(c => at(c).join() === RED.join())
  let eyeDistance = (c: TileChunk) => Math.hypot(c.x + CHUNK / 2 - EYE, c.y + CHUNK / 2 - EYE)
  let counters = () => app.stats().app as { tileBakes: number; tileBakesWaiting: number }
  expect(layer.resident().length).toBe(9)
  for (let c of layer.resident()) expect(at(c)).toEqual(WHITE)
  let baked = counters().tileBakes
  setBakeBudget(1)
  try {
    // One cell per resident chunk goes red: nine re-bakes, one a frame.
    for (let c of layer.resident()) layer.setTile(c.col, c.row, red)
    await app.frame()
    // The chunk under the eye first; every other still shows its last bake.
    expect(reds().map(c => `${c.col},${c.row}`)).toEqual(["0,0"])
    expect(counters().tileBakes).toBe(baked + 1)
    expect(counters().tileBakesWaiting).toBe(8)
    // Then the four a chunk away, before the four diagonal ones.
    await app.frame()
    expect(reds().length).toBe(2)
    await app.frame(3)
    expect(reds().length).toBe(5)
    for (let c of reds()) expect(eyeDistance(c) <= CHUNK).toBe(true)
    // The layer demands the frames the rest need: a settle finishes the roll.
    await app.settle()
    expect(reds().length).toBe(9)
    expect(counters().tileBakes).toBe(baked + 9)
    expect(counters().tileBakesWaiting).toBe(0)
    // The camera pans two chunks right: the column a chunk behind leaves
    // (past the leave band; the one half a chunk behind stays in it), six
    // chunks enter, and in the frame their enter lands the six resident
    // chunks left behind are dirtied too (from onEnter, so both happen in
    // one frame; the leaves came first in the slot). The entering chunks
    // bake in that frame (red: their own cells, not a pooled target's
    // white) and spend its budget, so its publish pass re-bakes nothing.
    // The flush the enters scheduled then runs at the microtask after the
    // frame and issues the NEXT frame's budget early: one re-bake, charged
    // to that frame, whose own publish pass issues none; the frame after
    // issues one.
    let dirtied = false
    layer.onEnter = () => {
      if (dirtied) return
      dirtied = true
      for (let c of layer.resident()) if (c.col < RED_FROM_COL) layer.setTile(c.col, c.row, white)
    }
    setCamera({ x: PACE_CAMERA.x + 2 * CHUNK, y: PACE_CAMERA.y })
    let start = counters().tileBakes
    let entered = false
    for (let frames = 0; frames < 4 && !entered; frames++) {
      await app.frame()
      let incoming = layer.resident().filter(c => c.col >= RED_FROM_COL)
      if (incoming.length === 0) {
        expect(counters().tileBakes).toBe(start)
        continue
      }
      entered = true
      expect(incoming.length).toBe(6)
      for (let c of incoming) expect(at(c)).toEqual(RED)
      expect(layer.resident().length).toBe(12)
      expect(layer.resident().some(c => c.col === -CHUNK_TILES)).toBe(false)
      expect(counters().tileBakes).toBe(start + 6 + 1)
      expect(counters().tileBakesWaiting).toBe(5)
    }
    expect(entered).toBe(true)
    await app.frame()
    expect(counters().tileBakes).toBe(start + 7)
    await app.frame()
    expect(counters().tileBakes).toBe(start + 8)
    expect(counters().tileBakesWaiting).toBe(4)
    // The layer demands the frames the rest need.
    await app.settle()
    for (let c of layer.resident()) expect(at(c)).toEqual(c.col >= RED_FROM_COL ? RED : WHITE)
    expect(counters().tileBakes).toBe(start + 12)
    expect(counters().tileBakesWaiting).toBe(0)
    expect(() => setBakeBudget(0)).toThrow()
    expect(() => setBakeBudget(1.5)).toThrow()
  } finally {
    setBakeBudget(DEFAULT_BAKE_BUDGET)
  }
})
