// Atlas creation: decoded pixels to a GPU texture plus the pixel size the
// slicers measure in. Thin by design - the frame math lives in frames.ts
// (pure, checkable) and the texture is an ordinary core texture. createImage
// is NOT used here: it never forwards sampler options, and pixel-art atlases
// want `filter: "nearest"` (render small, display big with hard pixels).
import { createTexture } from "@solidrt/core"
import type { DecodedImage } from "@solidrt/core"
import type { SamplerOptions, TextureId } from "@solidrt/core/gpu"

/**
 * A texture with its pixel size: what `grid` and `namedFrames` slice, and
 * whose `texture` a layer samples. A plain record, so a texture that did not
 * come through `createAtlas` - a render target, a camera frame - is an atlas
 * as a literal: `{ texture, width, height }`.
 */
export type Atlas = {
  /** The atlas texture, sampled by every sprite in layers created over it. */
  texture: TextureId
  /** Pixel size, the space the slicers measure rects and gaps in. */
  width: number
  height: number
}

/**
 * Sampling is core's `SamplerOptions`, fixed at creation like every core
 * texture: `filter` ("nearest" is the pixel-art path with hard pixels at any
 * scale, "linear" the default photographic one), `wrap`, `mipmap` (a chain
 * for atlases drawn far below their texel size) and `anisotropy`.
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
 * `await file("assets/sheet.png").bytes()` - and pixels built in code pass
 * as they are. Freed with the owning reactive scope like any core texture
 * (opt out with `{ autoFree: false }`).
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
