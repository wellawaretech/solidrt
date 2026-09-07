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

/**
 * What the slicers need of an atlas: its pixel size, the space every rect
 * and gap below is measured in. An `Atlas` fits as it is; a texture from
 * anywhere else (a render target, a camera) passes `{ width, height }`.
 */
export type AtlasSize = { width: number; height: number }

export type GridOptions = {
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
 *
 * Cells that touch bleed: a sprite at a fractional position samples, on
 * the odd frame, a texel of the cell next door along its edge (a one-texel
 * line of the neighbour, gone the next frame - see namedFrames). Pack the
 * sheet with a transparent gutter and pass it as `spacing`.
 */
export function grid(atlas: AtlasSize, cols: number, rows: number, opts?: GridOptions): Frame[] {
  if (!(cols > 0 && rows > 0 && Number.isInteger(cols) && Number.isInteger(rows))) {
    throw new Error(`grid: cols and rows must be positive integers, got ${cols} x ${rows}`)
  }
  let { width, height } = atlas
  if (!(width > 0 && height > 0)) {
    throw new Error(`grid: atlas size must be positive, got ${width} x ${height}`)
  }
  let { spacing = 0, marginX = 0, marginY = 0 } = opts ?? {}
  let cellW = opts?.cellW ?? (width - marginX * 2 - spacing * (cols - 1)) / cols
  let cellH = opts?.cellH ?? (height - marginY * 2 - spacing * (rows - 1)) / rows
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
 * hand-packed or tool-exported sheets. The key union comes from the table
 * you pass: an object literal (or a built table with `satisfies Record<...>`
 * on the literal) gives a record whose every name is checked; a table
 * typed `Record<string, ...>` gives back an index signature, so every name
 * compiles, a typo included, and no `!` is needed on a lookup - the
 * scaffold's tsconfig does not set noUncheckedIndexedAccess.
 *
 * The oldest atlas trap: frames addressed as whole-pixel rects that share
 * an edge bleed into each other. A sprite drifting by fractions of a pixel
 * lands, on the odd frame, on a sample position that rounds into the cell
 * next door and paints a one-texel line of it - a solid cell beside a
 * sprite flashes a bright bar over it for a single frame, invisible in a
 * screenshot and easy to blame on the game. Inset every rect half a texel
 * (`[x + 0.5, y + 0.5, w - 1, h - 1]`), or pack a transparent gutter
 * between cells and keep full-bleed cells in a corner of their own.
 */
export function namedFrames<K extends string>(
  atlas: AtlasSize,
  rects: Record<K, [number, number, number, number]>,
): Record<K, Frame> {
  let { width, height } = atlas
  if (!(width > 0 && height > 0)) {
    throw new Error(`namedFrames: atlas size must be positive, got ${width} x ${height}`)
  }
  let out = {} as Record<K, Frame>
  for (let name in rects) {
    let [x, y, w, h] = rects[name]
    if (!(w > 0 && h > 0)) throw new Error(`namedFrames: frame '${name}' has non-positive size ${w} x ${h}`)
    out[name] = { u0: x / width, v0: y / height, u1: (x + w) / width, v1: (y + h) / height }
  }
  return out
}

/** The whole texture as one frame (a plain image used as a sprite). */
export const FULL_FRAME: Frame = { u0: 0, v0: 0, u1: 1, v1: 1 }
