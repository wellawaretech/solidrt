// Atlas creation: decoded pixels to a GPU texture plus the pixel size the
// slicers measure in, and the checks a layer runs over the atlases it
// declares. Thin by design - the frame math lives in frames.ts (pure,
// checkable) and the texture is an ordinary core texture. createImage is
// NOT used here: it never forwards sampler options, and pixel-art atlases
// want `filter: "nearest"` (render small, display big with hard pixels).
import { createTexture } from "@solidrt/core"
import type { DecodedImage } from "@solidrt/core"
import { limits } from "@solidrt/core/gpu"
import type { SamplerOptions, TextureId } from "@solidrt/core/gpu"
import type { Frame } from "./frames.ts"

/**
 * A texture with its pixel size: what `grid`, `namedFrames` and
 * `fullFrame` slice (stamping the texture on every frame), and what a
 * layer declares in its `atlases` list. A plain record, so a texture that
 * did not come through `createAtlas` - a render target, a camera frame -
 * is an atlas as a literal: `{ texture, width, height }`.
 */
export type Atlas = {
  /** The atlas texture: what the frames cut from it name, and what a layer
   * declaring this atlas binds. */
  texture: TextureId
  /** Pixel size, the space the slicers measure rects and gaps in. */
  width: number
  height: number
  /**
   * The sheet is a distance field, not colour: a multi-channel signed
   * distance field in rgb (the edge is the median at 0.5) with the true
   * field in alpha, `range` texels wide (what `flux:font`'s msdf atlas
   * holds, see createSpriteFont). A layer declaring it decodes the field
   * in its fragment stage - sharp edges at any zoom from one sheet, and
   * the per-sprite outline - instead of sampling colour; the sprite's
   * tint is the fill colour. Fixed at the layer's creation like the list
   * itself: the fragment stage is generated per layer.
   */
  sdf?: { range: number }
}

/**
 * Sampling is core's `SamplerOptions`, fixed at creation like every core
 * texture: `filter` ("nearest" is the pixel-art path with hard pixels at any
 * scale, "linear" the default photographic one), `wrap`, `mipmap` (a chain
 * for atlases drawn far below their texel size - extrude the sheet first,
 * see extrude.ts) and `anisotropy`.
 */
export type AtlasOptions = SamplerOptions & {
  label?: string
  /** Skip the owner-scoped auto-free (the core createTexture contract). */
  autoFree?: boolean
}

/**
 * Upload decoded pixels as an atlas texture. `image` is a core
 * `DecodedImage` (`{ data, width, height }`, premultiplied RGBA8): an
 * encoded sheet goes through `decodeImage` first -
 * `createAtlas(decodeImage(sheet), { filter: "nearest" })`, with `sheet`
 * from `import sheet from "./sheet.png" with { type: "binary" }` or
 * `await file("assets/sheet.png").bytes()` - and pixels built in code
 * (or repacked by `extrudeGrid`/`extrudeRects`) pass as they are. Freed
 * with the owning reactive scope like any core texture (opt out with
 * `{ autoFree: false }`).
 */
export function createAtlas(image: DecodedImage, opts?: AtlasOptions): Atlas {
  let { data, width, height } = image
  let texture = createTexture(data, width, height, {
    ...opts,
    filter: opts?.filter ?? "linear",
    label: opts?.label ?? "atlas",
  })
  return { texture, width, height }
}

/** Whether a value is an atlas record: a non-negative integer texture id
 * and a positive pixel size. */
export function isAtlas(a: unknown): a is Atlas {
  let o = a as Atlas | null
  return typeof o === "object" && o !== null && Number.isInteger(o.texture) && o.texture >= 0 && o.width > 0 && o.height > 0
}

/**
 * Validate a layer's `atlases` list (throws - the dev validation policy):
 * a non-empty array of distinct atlas records, no more than the device
 * binds in one pass (`limits.maxTextureUnits`, 16 on every GLES 3.0
 * device). Returns the texture -> sampler index map the layer resolves
 * frames through; the index is the atlas's position in the list.
 * Internal - every layer kind calls it.
 */
export function checkAtlases(verb: string, atlases: readonly Atlas[]): Map<TextureId, number> {
  if (!Array.isArray(atlases) || atlases.length === 0) {
    throw new Error(`${verb}: atlases must be a non-empty array of atlas records (createAtlas, or { texture, width, height })`)
  }
  if (atlases.length > limits.maxTextureUnits) {
    throw new Error(`${verb}: ${atlases.length} atlases exceed the ${limits.maxTextureUnits} sampler inputs one pass may bind (limits.maxTextureUnits); pack sheets together`)
  }
  let index = new Map<TextureId, number>()
  atlases.forEach((atlas, i) => {
    if (!isAtlas(atlas)) throw new Error(`${verb}: atlases[${i}] is not an atlas record, got ${JSON.stringify(atlas)}`)
    if (index.has(atlas.texture)) throw new Error(`${verb}: atlases[${i}] declares texture ${atlas.texture} a second time`)
    index.set(atlas.texture, i)
  })
  return index
}

/**
 * The sampler index of a frame's texture in a layer's atlas list, or a
 * throw naming the list: a frame from a sheet the layer did not declare
 * is the wrong-sheet bug, caught at the write instead of drawn from the
 * wrong texture. Internal - the layers' frame writes go through it.
 */
export function frameIndex(verb: string, index: Map<TextureId, number>, frame: Frame): number {
  let i = index.get(frame.texture)
  if (i === undefined) {
    throw new Error(`${verb}: the frame's texture ${frame.texture} is not one of the layer's atlases (${[...index.keys()].join(", ")}); frames come from grid/namedFrames/fullFrame over a declared atlas`)
  }
  return i
}
