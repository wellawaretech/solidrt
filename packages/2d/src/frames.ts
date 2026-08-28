// Atlas frame math, pure: names and grid coordinates to normalized UV rects.
// No GPU or GUI imports, so the checks rig (checks/frames-check.ts) exercises
// this module headless on the flux binary.

/**
 * One atlas frame as normalized UVs: the rect [u0, v0] to [u1, v1] with
 * top-left origin (the texture pixel contract), u right, v down.
 */
export type Frame = { u0: number; v0: number; u1: number; v1: number }

/**
 * Write a frame's four UVs at `at`, mirrored on the UV side when flipped:
 * `flipX` swaps u0/u1, `flipY` swaps v0/v1. A flip is an involution, so the
 * same call with the stored floats and only the CHANGED axes set toggles a
 * flip in place, and reading back through it recovers the frame. Positional
 * floats so the write allocates nothing.
 */
export function writeFrame(
  data: Float32Array,
  at: number,
  u0: number,
  v0: number,
  u1: number,
  v1: number,
  flipX: boolean,
  flipY: boolean,
): void {
  data[at] = flipX ? u1 : u0
  data[at + 1] = flipY ? v1 : v0
  data[at + 2] = flipX ? u0 : u1
  data[at + 3] = flipY ? v0 : v1
}

export type GridOptions = {
  /** Pixel size of the atlas the pixel-space options below refer to. */
  width: number
  height: number
  /** Cell size in pixels; defaults to width/cols x height/rows. */
  cellW?: number
  cellH?: number
  /** Pixel gap between cells (not around the edge); default 0. */
  spacing?: number
  /** Pixel offset of the first cell from the top-left corner; default 0. */
  marginX?: number
  marginY?: number
}

/**
 * Slice a uniform sprite sheet into frames, row-major (left to right, then
 * top to bottom) - the layout every sheet packer and pixel-art tool emits.
 * Frames are returned in cell order, so `frames[row * cols + col]` addresses
 * a cell and an animation is a slice of consecutive indices.
 */
export function grid(cols: number, rows: number, opts: GridOptions): Frame[] {
  if (!(cols > 0 && rows > 0 && Number.isInteger(cols) && Number.isInteger(rows))) {
    throw new Error(`grid: cols and rows must be positive integers, got ${cols} x ${rows}`)
  }
  let { width, height, spacing = 0, marginX = 0, marginY = 0 } = opts
  if (!(width > 0 && height > 0)) {
    throw new Error(`grid: atlas size must be positive, got ${width} x ${height}`)
  }
  let cellW = opts.cellW ?? (width - marginX * 2 - spacing * (cols - 1)) / cols
  let cellH = opts.cellH ?? (height - marginY * 2 - spacing * (rows - 1)) / rows
  if (!(cellW > 0 && cellH > 0)) {
    throw new Error(`grid: derived cell size ${cellW} x ${cellH} is not positive`)
  }
  let frames: Frame[] = []
  for (let row = 0; row < rows; row++) {
    for (let col = 0; col < cols; col++) {
      let x = marginX + col * (cellW + spacing)
      let y = marginY + row * (cellH + spacing)
      frames.push({ u0: x / width, v0: y / height, u1: (x + cellW) / width, v1: (y + cellH) / height })
    }
  }
  return frames
}

/**
 * Name frames from a pixel-rect map: `{ hero: [x, y, w, h], ... }` in atlas
 * pixels to `{ hero: Frame, ... }`. The named counterpart of `grid` for
 * hand-packed or tool-exported sheets.
 */
export function namedFrames<K extends string>(
  atlasW: number,
  atlasH: number,
  rects: Record<K, [number, number, number, number]>,
): Record<K, Frame> {
  if (!(atlasW > 0 && atlasH > 0)) {
    throw new Error(`namedFrames: atlas size must be positive, got ${atlasW} x ${atlasH}`)
  }
  let out = {} as Record<K, Frame>
  for (let name in rects) {
    let [x, y, w, h] = rects[name]
    if (!(w > 0 && h > 0)) throw new Error(`namedFrames: frame '${name}' has non-positive size ${w} x ${h}`)
    out[name] = { u0: x / atlasW, v0: y / atlasH, u1: (x + w) / atlasW, v1: (y + h) / atlasH }
  }
  return out
}

/** The whole texture as one frame (a plain image used as a sprite). */
export const FULL_FRAME: Frame = { u0: 0, v0: 0, u1: 1, v1: 1 }
