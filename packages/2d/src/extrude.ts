// Atlas extrusion, pure: repack a sheet so every cell sits in a gutter of
// its own replicated edge pixels - TexturePacker's "Extrude", Unity's
// "Extrude Edges" import setting - the answer to mip-chain bleed. The
// layer shaders clamp every sample into its frame, which keeps mip level 0
// clean on an edge-to-edge sheet, but a mip chain averages 2^k texel
// blocks BEFORE any sampling decision, and a block straddling a cell edge
// mixes two cells. With a gutter of g replicated texels around every cell
// and every cell edge on a multiple of g, each block at level k <= log2(g)
// lies inside one cell's own colours, so levels 0 through log2(g) sample
// clean (deeper levels mix neighbours again; widen the gutter for a sheet
// drawn further below its texel size). CPU work at load time - a few
// milliseconds for a 1024-square sheet - and the result goes through
// createAtlas like any decoded image. No GPU or GUI imports (the
// DecodedImage import is a type), so tests/extrude.test.ts runs headless.
import type { DecodedImage } from "@solidrt/core"
import type { GridOptions } from "./frames.ts"
import { packRects, packWidth } from "./pack.ts"

// Bytes per pixel of a DecodedImage (tightly packed RGBA8).
const BYTES_PER_PIXEL = 4

/** A named pixel-rect table, `[x, y, w, h]` per name: namedFrames' input. */
export type Rects<K extends string> = Record<K, [number, number, number, number]>

/** extrudeGrid's result: the repacked sheet and the GridOptions that slice it. */
export type ExtrudedGrid = { image: DecodedImage; options: GridOptions }
/** extrudeRects' result: the repacked sheet and each rect's place in it. */
export type ExtrudedRects<K extends string> = { image: DecodedImage; rects: Rects<K> }

/**
 * Extrude a uniform sheet: every cell of the `cols` x `rows` grid (laid
 * out by `source`, the same geometry `grid` reads - cell size, spacing,
 * margins; default the sheet divided evenly) is copied into a new sheet
 * inside a `gutter`-texel border of its own replicated edge pixels.
 * Returns the sheet and the GridOptions that slice it:
 * `grid(atlas, cols, rows, extruded.options)` over
 * `createAtlas(extruded.image, { mipmap: true })`.
 *
 * `gutter` is a power of two and the cell size a multiple of it, so cell
 * edges land on mip texel edges; mip levels 0 through log2(gutter) then
 * sample clean. The new sheet is `cell + 2 * gutter` per cell on each
 * axis - a 16 px cell with a 4-texel gutter is 24 px, 2.25x the texels.
 * Throws on a gutter or cell size that breaks the alignment, or a grid
 * the sheet does not hold.
 */
export function extrudeGrid(image: DecodedImage, cols: number, rows: number, gutter: number, source?: GridOptions): ExtrudedGrid {
  checkImage("extrudeGrid", image)
  if (!(cols > 0 && rows > 0 && Number.isInteger(cols) && Number.isInteger(rows))) {
    throw new Error(`extrudeGrid: cols and rows must be positive integers, got ${cols} x ${rows}`)
  }
  checkGutter("extrudeGrid", gutter)
  let { width, height } = image
  let { spacing = 0, marginX = 0, marginY = 0 } = source ?? {}
  let cellW = source?.cellW ?? (width - marginX * 2 - spacing * (cols - 1)) / cols
  let cellH = source?.cellH ?? (height - marginY * 2 - spacing * (rows - 1)) / rows
  checkCell("extrudeGrid", cellW, cellH, gutter)
  if (marginX + cols * cellW + (cols - 1) * spacing > width || marginY + rows * cellH + (rows - 1) * spacing > height) {
    throw new Error(`extrudeGrid: a ${cols} x ${rows} grid of ${cellW} x ${cellH} cells does not fit the ${width} x ${height} sheet`)
  }
  let pitchX = cellW + 2 * gutter
  let pitchY = cellH + 2 * gutter
  let out = blank(cols * pitchX, rows * pitchY)
  for (let row = 0; row < rows; row++) {
    for (let col = 0; col < cols; col++) {
      copyCell(image, marginX + col * (cellW + spacing), marginY + row * (cellH + spacing), cellW, cellH, out, col * pitchX + gutter, row * pitchY + gutter, gutter)
    }
  }
  return { image: out, options: { cellW, cellH, spacing: 2 * gutter, marginX: gutter, marginY: gutter } }
}

/**
 * Extrude a hand-packed sheet: every named rect is re-placed on a new
 * sheet (a shelf packing, pack.ts) inside a `gutter`-texel border of its
 * own replicated edge pixels, and the table of new rects comes back for
 * `namedFrames(atlas, extruded.rects)`. The same alignment rule as
 * extrudeGrid: `gutter` a power of two, every rect's size a multiple of
 * it (pad a rect in the source sheet to make it so). The new sheet is a
 * power of two wide and as tall as its shelves.
 */
export function extrudeRects<K extends string>(image: DecodedImage, rects: Rects<K>, gutter: number): ExtrudedRects<K> {
  checkImage("extrudeRects", image)
  checkGutter("extrudeRects", gutter)
  let names = Object.keys(rects) as K[]
  if (names.length === 0) throw new Error("extrudeRects: no rects to extrude")
  let sizes = names.map(name => {
    let [x, y, w, h] = rects[name]
    checkCell(`extrudeRects: rect '${name}'`, w, h, gutter)
    if (!(Number.isInteger(x) && Number.isInteger(y) && x >= 0 && y >= 0 && x + w <= image.width && y + h <= image.height)) {
      throw new Error(`extrudeRects: rect '${name}' [${x}, ${y}, ${w}, ${h}] is not inside the ${image.width} x ${image.height} sheet`)
    }
    return { width: w + 2 * gutter, height: h + 2 * gutter }
  })
  let packing = packRects(sizes, packWidth(sizes))
  let out = blank(packing.width, packing.height)
  let placed = {} as Rects<K>
  names.forEach((name, i) => {
    let [x, y, w, h] = rects[name]
    let p = packing.rects[i]!
    copyCell(image, x, y, w, h, out, p.x + gutter, p.y + gutter, gutter)
    placed[name] = [p.x + gutter, p.y + gutter, w, h]
  })
  return { image: out, rects: placed }
}

/** A transparent sheet of the given size. */
function blank(width: number, height: number): DecodedImage {
  return { data: new Uint8Array(width * height * BYTES_PER_PIXEL), width, height }
}

/**
 * Copy the `w` x `h` cell at (sx, sy) of `src` to (dx, dy) of `dst` inside
 * a `g`-texel border replicating its edge pixels: each output row takes
 * the nearest cell row in one bulk copy, then its first and last pixels
 * fill the side gutters pixel by pixel.
 */
function copyCell(src: DecodedImage, sx: number, sy: number, w: number, h: number, dst: DecodedImage, dx: number, dy: number, g: number): void {
  let srcRow = src.width * BYTES_PER_PIXEL
  let dstRow = dst.width * BYTES_PER_PIXEL
  let cellBytes = w * BYTES_PER_PIXEL
  for (let oy = -g; oy < h + g; oy++) {
    let row = sy + Math.min(Math.max(oy, 0), h - 1)
    let from = row * srcRow + sx * BYTES_PER_PIXEL
    let to = (dy + oy) * dstRow + dx * BYTES_PER_PIXEL
    dst.data.set(src.data.subarray(from, from + cellBytes), to)
    let last = to + cellBytes - BYTES_PER_PIXEL
    for (let i = 1; i <= g; i++) {
      dst.data.copyWithin(to - i * BYTES_PER_PIXEL, to, to + BYTES_PER_PIXEL)
      dst.data.copyWithin(last + i * BYTES_PER_PIXEL, last, last + BYTES_PER_PIXEL)
    }
  }
}

function checkImage(what: string, image: DecodedImage): void {
  let { width, height, data } = image
  if (!(Number.isInteger(width) && Number.isInteger(height) && width > 0 && height > 0)) {
    throw new Error(`${what}: image size must be positive integers, got ${width} x ${height}`)
  }
  if (data.length !== width * height * BYTES_PER_PIXEL) {
    throw new Error(`${what}: image data holds ${data.length} bytes, not the ${width * height * BYTES_PER_PIXEL} of ${width} x ${height} RGBA8 pixels`)
  }
}

function checkGutter(what: string, gutter: number): void {
  if (!(Number.isInteger(gutter) && gutter >= 1 && (gutter & (gutter - 1)) === 0)) {
    throw new Error(`${what}: gutter must be a power of two >= 1, got ${gutter}`)
  }
}

function checkCell(what: string, w: number, h: number, gutter: number): void {
  if (!(Number.isInteger(w) && Number.isInteger(h) && w > 0 && h > 0)) {
    throw new Error(`${what}: cell size must be positive integers, got ${w} x ${h}`)
  }
  if (w % gutter !== 0 || h % gutter !== 0) {
    throw new Error(`${what}: a ${w} x ${h} cell is not a multiple of the ${gutter}-texel gutter, so its edges would not land on mip texel edges; pad the cell or lower the gutter`)
  }
}
