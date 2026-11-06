// Atlas creation: decoded pixels to a GPU texture plus the pixel size the
// slicers measure in, and the checks a layer runs over the atlases it
// declares. Thin by design - the frame math lives in frames.ts (pure,
// checkable) and the texture is an ordinary core texture. createImage is
// NOT used here: it never forwards sampler options, and pixel-art atlases
// want `filter: "nearest"` (render small, display big with hard pixels).
import { createTexture } from "@solidrt/core"
import type { DecodedImage } from "@solidrt/core"
import { limits } from "@solidrt/core/gpu"
import type { FilterMode, SamplerOptions, TextureId } from "@solidrt/core/gpu"
import type { Frame } from "./frames.ts"
import { coverageMask } from "./sprite-mesh.ts"
import type { AtlasMesh } from "./sprite-mesh.ts"

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
  /**
   * The sampler filter the texture was created with, as a plain fact:
   * createAtlas records it, and nothing renders from it (the texture
   * carries its own sampler). A view of a sprite or record layer over a
   * "nearest" atlas warns once when its oversample is fractional, since
   * the texels then come out of uneven widths (set `oversampleStep={1}`).
   * A literal states it to get the same check; absent, no check.
   */
  filter?: FilterMode
  /** Whether the texture has a mip chain, as a plain fact: createAtlas
   * records it, and a hull layer over it pushes its meshes further out,
   * since a minified tap blurs past the shape (MESH_MIP_EDGE_PX). A
   * literal states it for the same; absent, no chain. */
  mipmap?: boolean
  /**
   * The sheet's mesh source (`createAtlas(image, { mesh: "hull" })`): the
   * coverage of its texels, kept for the atlas's life so the slicers fill
   * `Frame.mesh` whenever frames are cut. A sprite or record layer
   * declaring such an atlas is a HULL layer: it draws each sprite as its
   * frame's mesh instead of the quad (a frame without a mesh still draws
   * its quad). A tile layer bakes quads either way. Fixed at the layer's
   * creation like the list itself.
   */
  mesh?: AtlasMesh
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
  /**
   * `"hull"`: keep the sheet's coverage (one bit per texel, 512 KiB for a
   * 2048 x 2048 sheet) so every frame cut from it carries a tight convex
   * mesh, and a layer declaring it shades the shape instead of its whole
   * frame quad - a sheet of irregular shapes spends about half its
   * fragments on transparent corners otherwise. Opt-in: a material that
   * draws outside the shape (a glow, a bevel, a drop shadow inside the
   * frame) would be cut, and a rectangular sprite gains nothing.
   */
  mesh?: "hull"
  /** The sheet is a distance field (see `Atlas.sdf`). With `mesh` the
   * coverage is the field's edge, not alpha above zero. */
  sdf?: { range: number }
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
  let { mesh, sdf, ...sampler } = opts ?? {}
  if (mesh !== undefined && mesh !== "hull") throw new Error(`createAtlas: mesh must be "hull", got ${JSON.stringify(mesh)}`)
  if (sdf !== undefined && !(typeof sdf.range === "number" && sdf.range > 0 && Number.isFinite(sdf.range))) {
    throw new Error(`createAtlas: sdf.range must be a positive number, got ${JSON.stringify(sdf.range)}`)
  }
  let texture = createTexture(data, width, height, {
    ...sampler,
    filter: opts?.filter ?? "linear",
    label: opts?.label ?? "atlas",
  })
  let atlas: Atlas = { texture, width, height, filter: opts?.filter ?? "linear", mipmap: opts?.mipmap === true }
  if (sdf !== undefined) atlas.sdf = { range: sdf.range }
  if (mesh !== undefined) atlas.mesh = { kind: mesh, coverage: coverageMask(data, width, height, sdf !== undefined) }
  return atlas
}

/** Whether a layer over `atlases` draws meshes (a HULL layer): any of
 * them keeps a mesh source. Internal - the sprite and record layers pick
 * their record layout and pipeline by it. */
export function hasMeshAtlas(atlases: readonly Atlas[]): boolean {
  return atlases.some(a => a.mesh !== undefined)
}

/** Whether a layer over `atlases` samples any of them nearest (the
 * recorded `filter`), which a fractional view oversample draws as blocks
 * of uneven widths. Internal - the sprite and record layers hand it to
 * their views. */
export function hasNearestAtlas(atlases: readonly Atlas[]): boolean {
  return atlases.some(a => a.filter === "nearest")
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
 * device) once the material's own `samplers` (its textures: a palette
 * LUT, a noise texture) are counted in - the two lists bind on the same
 * targets. Returns the texture -> sampler index map the layer resolves
 * frames through; the index is the atlas's position in the list.
 * Internal - every layer kind calls it.
 */
export function checkAtlases(verb: string, atlases: readonly Atlas[], samplers = 0): Map<TextureId, number> {
  if (!Array.isArray(atlases) || atlases.length === 0) {
    throw new Error(`${verb}: atlases must be a non-empty array of atlas records (createAtlas, or { texture, width, height })`)
  }
  if (atlases.length + samplers > limits.maxTextureUnits) {
    throw new Error(
      `${verb}: ${atlases.length} atlases${samplers > 0 ? ` plus the material's ${samplers} textures` : ""} exceed the ${limits.maxTextureUnits} sampler inputs one pass may bind (limits.maxTextureUnits); pack sheets together`,
    )
  }
  let index = new Map<TextureId, number>()
  atlases.forEach((atlas, i) => {
    if (!isAtlas(atlas)) throw new Error(`${verb}: atlases[${i}] is not an atlas record, got ${JSON.stringify(atlas)}`)
    if (index.has(atlas.texture)) throw new Error(`${verb}: atlases[${i}] declares texture ${atlas.texture} a second time`)
    if (atlas.filter !== undefined && atlas.filter !== "linear" && atlas.filter !== "nearest") {
      throw new Error(`${verb}: atlases[${i}].filter must be "linear" or "nearest", got ${JSON.stringify(atlas.filter)}`)
    }
    let mesh = atlas.mesh
    if (mesh !== undefined && !(mesh.kind === "hull" && mesh.coverage instanceof Uint32Array && mesh.coverage.length * 32 >= atlas.width * atlas.height)) {
      throw new Error(`${verb}: atlases[${i}].mesh is not a coverage mask of the atlas's texels (createAtlas with mesh: "hull")`)
    }
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
