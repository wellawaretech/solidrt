// @solidrt/core/textures - what a texture bake and a loader share, without
// the runtime: this entry imports nothing the runtime provides, so a bake
// script under bun and an extension's pure modules use it as it is
// (tests/textures.test.ts imports it under bun and fails the moment a
// runtime import creeps in). Any extension that bakes or loads textures
// builds on this, the first-party 3d one included.
//
// isKtx2 tells a compressed texture file from a PNG or JPEG.
//
// textureSettings reads how an app's textures are compressed at bake
// time: the `textures` group of the `solidrt` key in its package.json, per
// kind of map.
//
//   "solidrt": { "textures": { "data": { "codec": "uastc", "quality": 0.9 } } }
//
// A kind left out keeps its default, and so does a field left out of a
// kind; anything unknown throws naming the key.

import type { TextureCodec } from "flux:image"

// The first twelve bytes of every KTX2 file.
const KTX2_MAGIC = [0xab, 0x4b, 0x54, 0x58, 0x20, 0x32, 0x30, 0xbb, 0x0d, 0x0a, 0x1a, 0x0a]

/** Whether `bytes` starts as a KTX2 file, the compressed texture
 * `transcodeTexture` takes: what tells one from a PNG or JPEG. */
export function isKtx2(bytes: Uint8Array): boolean {
  return bytes.length >= KTX2_MAGIC.length && KTX2_MAGIC.every((b, i) => bytes[i] === b)
}

/**
 * What the texels of a texture ARE, which decides how it may be
 * compressed. Not what draws it: an extension maps its own uses (a
 * material's slots, a sprite sheet, a terrain's layers) onto these.
 *
 * - `color`: color as the eye sees it, sRGB-encoded. A photo, a painted
 *   surface, a sprite, a tile set.
 * - `normal`: directions, unit vectors packed into the channels. An error
 *   tilts the lighting computed from them.
 * - `data`: any other linear values that weigh a computation and tolerate
 *   a small error: a mask, a roughness, a height for parallax.
 *
 * Values that must come back EXACT are none of these and are not block
 * compressed at all, at any setting: a lookup table, a distance field, an
 * index map. The same goes for sharp art and text.
 */
export type TextureKind = "color" | "normal" | "data"

export type TextureSetting = {
  codec: TextureCodec
  /** 0..1, higher is larger and closer to the source. */
  quality: number
}

export type TextureSettings = Record<TextureKind, TextureSetting>

const CODECS: TextureCodec[] = ["etc1s", "uastc"]

// The defaults, by what each kind tolerates. Color and data take the small
// codec (ETC1S moves little across its scale, so it sits high); directions
// show their errors as blotches in the lighting, so they take the accurate
// one, at the quality below which it gives up detail for almost no bytes.
const SMALL_CODEC_QUALITY = 0.75
const ACCURATE_CODEC_QUALITY = 0.9

export const DEFAULT_TEXTURE_SETTINGS: TextureSettings = {
  color: { codec: "etc1s", quality: SMALL_CODEC_QUALITY },
  normal: { codec: "uastc", quality: ACCURATE_CODEC_QUALITY },
  data: { codec: "etc1s", quality: SMALL_CODEC_QUALITY },
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

/**
 * The texture settings of a parsed package.json: its `solidrt.textures`
 * group over the defaults. A package.json without the group (or none at
 * all: pass undefined) gives the defaults. Throws on an unknown kind or
 * field, a codec that does not exist, a quality outside 0..1.
 */
export function textureSettings(packageJson: unknown): TextureSettings {
  let settings: TextureSettings = {
    color: { ...DEFAULT_TEXTURE_SETTINGS.color },
    normal: { ...DEFAULT_TEXTURE_SETTINGS.normal },
    data: { ...DEFAULT_TEXTURE_SETTINGS.data },
  }
  let solidrt = isRecord(packageJson) ? packageJson.solidrt : undefined
  let group = isRecord(solidrt) ? solidrt.textures : undefined
  if (group === undefined) return settings
  if (!isRecord(group)) throw new Error("solidrt.textures must be an object of texture kinds (color, normal, data)")
  for (let [kind, value] of Object.entries(group)) {
    if (!(kind in settings)) throw new Error(`solidrt.textures.${kind}: unknown texture kind (expected color, normal or data)`)
    if (!isRecord(value)) throw new Error(`solidrt.textures.${kind} must be an object with codec and quality`)
    let setting = settings[kind as TextureKind]
    for (let [field, given] of Object.entries(value)) {
      let key = `solidrt.textures.${kind}.${field}`
      if (field === "codec") {
        if (!CODECS.includes(given as TextureCodec)) throw new Error(`${key}: unknown codec ${JSON.stringify(given)} (expected "etc1s" or "uastc")`)
        setting.codec = given as TextureCodec
      } else if (field === "quality") {
        if (typeof given !== "number" || !(given >= 0 && given <= 1)) throw new Error(`${key}: ${JSON.stringify(given)} is not a number in 0..1`)
        setting.quality = given
      } else {
        throw new Error(`${key}: unknown field (expected codec or quality)`)
      }
    }
  }
  return settings
}
