// Atlas creation: encoded image bytes to a GPU texture plus its frame table.
// Thin by design - the frame math lives in frames.ts (pure, checkable) and
// the texture is an ordinary core texture. createImage is NOT used here: it
// never forwards sampler options, and pixel-art atlases want
// `filter: "nearest"` (render small, display big with hard pixels).
import { createTexture, decodeImage } from "@solidrt/core"
import type { TextureId } from "@solidrt/core/gpu"
import type { Frame } from "./frames.ts"
import { grid } from "./frames.ts"
import type { GridOptions } from "./frames.ts"

export type Atlas = {
  /** The atlas texture, sampled by every sprite in layers created over it. */
  texture: TextureId
  width: number
  height: number
  /** Frames in cell order when created via the grid option, else empty. */
  frames: Frame[]
}

export type AtlasOptions = {
  /**
   * Sampling: "nearest" is the pixel-art path (hard pixels at any scale),
   * "linear" (default) the photographic one. Fixed at creation, like every
   * core texture.
   */
  filter?: "nearest" | "linear"
  /** Slice a uniform sheet at creation: cols x rows of equal cells. */
  grid?: { cols: number; rows: number } & Omit<GridOptions, "width" | "height">
  label?: string
  /** Skip the owner-scoped auto-free (the core createTexture contract). */
  autoFree?: boolean
}

/**
 * Decode encoded image bytes (PNG, JPEG, ...) into an atlas texture. Bytes
 * come from `import sheet from "./sheet.png" with { type: "binary" }` or
 * `await file("assets/sheet.png").bytes()`. Freed with the owning reactive
 * scope like any core texture (opt out with `{ autoFree: false }`).
 */
export function createAtlas(bytes: Uint8Array, opts?: AtlasOptions): Atlas {
  let decoded = decodeImage(bytes)
  let texture = createTexture(decoded.data, decoded.width, decoded.height, {
    filter: opts?.filter ?? "linear",
    label: opts?.label ?? "atlas",
    autoFree: opts?.autoFree,
  })
  let frames = opts?.grid
    ? grid(opts.grid.cols, opts.grid.rows, { ...opts.grid, width: decoded.width, height: decoded.height })
    : []
  return { texture, width: decoded.width, height: decoded.height, frames }
}
