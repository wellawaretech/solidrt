// Atlas frame math, pure: names and grid coordinates to normalized UV rects,
// each stamped with the texture it is cut from. No GPU or GUI imports (the
// Atlas and TextureId imports are types), so its test
// (tests/frames.test.ts) exercises this module headless on the flux binary.
import type { TextureId } from "@solidrt/core/gpu"
import type { Atlas } from "./atlas.ts"

/**
 * One atlas frame: the texture it is cut from plus the rect [u0, v0] to
 * [u1, v1] in normalized UVs, top-left origin (the texture pixel
 * contract), u right, v down. A frame is self-describing - PixiJS's
 * Texture, Phaser's Frame, Unity's Sprite and Godot's AtlasTexture all
 * pair the texture with the rect - so the same frame draws in every layer
 * that declares its atlas, and a layer that does not rejects it at the
 * write (the wrong-sheet bug, caught instead of drawn).
 */
export type Frame = { texture: TextureId; u0: number; v0: number; u1: number; v1: number }

/** Whether a value is a frame: a non-negative integer texture id and four
 * numeric UVs (what the layers check before touching a record). */
export function isFrame(f: unknown): f is Frame {
  let o = f as Frame | null
  return (
    typeof o === "object" &&
    o !== null &&
    Number.isInteger(o.texture) &&
    o.texture >= 0 &&
    typeof o.u0 === "number" &&
    typeof o.v0 === "number" &&
    typeof o.u1 === "number" &&
    typeof o.v1 === "number"
  )
}

/**
 * Write a frame's four UVs at `at`, mirrored on the UV side when flipped:
 * `flipX` swaps u0/u1, `flipY` swaps v0/v1. A flip is an involution, so the
 * same call with the stored floats and only the CHANGED axes set toggles a
 * flip in place, and reading back through it recovers the frame. Positional
 * floats so the write allocates nothing; the texture's sampler index is the
 * layer's own field beside these four.
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
 * The geometry of a uniform sheet: what `grid` reads to place cells, and
 * what `extrudeGrid` returns for the sheet it repacks.
 */
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

/** The slicers' atlas check: a positive pixel size and a texture id to
 * stamp (throws - the dev validation policy). */
function checkAtlas(what: string, atlas: Atlas): void {
  if (!(atlas.width > 0 && atlas.height > 0)) {
    throw new Error(`${what}: atlas size must be positive, got ${atlas.width} x ${atlas.height}`)
  }
  if (!(Number.isInteger(atlas.texture) && atlas.texture >= 0)) {
    throw new Error(`${what}: atlas has no texture id, got ${JSON.stringify(atlas.texture)}`)
  }
}

/** A pixel rect of `atlas` to a frame over its texture. */
function frameOf(atlas: Atlas, x: number, y: number, w: number, h: number): Frame {
  let { width, height, texture } = atlas
  return { texture, u0: x / width, v0: y / height, u1: (x + w) / width, v1: (y + h) / height }
}

/**
 * Slice a uniform sprite sheet into frames, row-major (left to right, then
 * top to bottom) - the layout every sheet packer and pixel-art tool emits.
 * Frames are returned in cell order, so `frames[row * cols + col]` addresses
 * a cell and an animation is a slice of consecutive indices.
 *
 * Cells that touch do not bleed at mip level 0: the layer shaders clamp
 * every sample into its frame (shaders.ts). A mip chain is different - its
 * texels straddle cell edges before sampling - so a sheet going through
 * `createAtlas` with `mipmap` is extruded first (`extrudeGrid`, whose
 * returned options slice the repacked sheet).
 */
export function grid(atlas: Atlas, cols: number, rows: number, opts?: GridOptions): Frame[] {
  if (!(cols > 0 && rows > 0 && Number.isInteger(cols) && Number.isInteger(rows))) {
    throw new Error(`grid: cols and rows must be positive integers, got ${cols} x ${rows}`)
  }
  checkAtlas("grid", atlas)
  let { width, height } = atlas
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
      frames.push(frameOf(atlas, x, y, cellW, cellH))
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
 * The oldest atlas trap - frames addressed as whole-pixel rects that share
 * an edge bleed into each other, a sprite drifting by fractions of a pixel
 * flashing a one-texel line of the cell next door - is handled in the
 * layer shaders, which clamp every sample into its frame. What they cannot
 * handle is a mip chain, whose texels average across cell edges before
 * sampling: a sheet uploaded with `mipmap` is extruded first
 * (`extrudeRects`, whose returned table names the repacked rects).
 */
export function namedFrames<K extends string>(atlas: Atlas, rects: Record<K, [number, number, number, number]>): Record<K, Frame> {
  checkAtlas("namedFrames", atlas)
  let out = {} as Record<K, Frame>
  for (let name in rects) {
    let [x, y, w, h] = rects[name]
    if (!(w > 0 && h > 0)) throw new Error(`namedFrames: frame '${name}' has non-positive size ${w} x ${h}`)
    out[name] = frameOf(atlas, x, y, w, h)
  }
  return out
}

/** The whole texture as one frame (a plain image used as a sprite, a
 * render target shown by a sprite). */
export function fullFrame(atlas: Atlas): Frame {
  checkAtlas("fullFrame", atlas)
  return { texture: atlas.texture, u0: 0, v0: 0, u1: 1, v1: 1 }
}
