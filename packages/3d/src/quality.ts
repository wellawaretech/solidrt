// The package's quality table: one row per app tier (core's `quality()`,
// the app's statement of the device's budget) holding every quality
// knob's default, each knob's explicit form the override. Unity's quality
// level asset holds the same rows (shadow resolution and cascades, the
// soft shadow tent, LOD bias and cross-fade, anti-aliasing, render scale,
// texture quality, anisotropic filtering); Godot keeps them as per-feature
// project settings with a `.mobile` override of each; Three has nothing.
// Here they are one table, and a quality is a row: a name picks a row
// of the table, a row object with fields missing takes them from the app
// tier's row (Unity's custom levels, in the one rule every knob follows).
//
// The rows are measured, not guessed. Low is the SM-T500 (Adreno 610) and
// Mali-T860 TV class, high the laptop and Pixel 7 class; the evidence per
// row is in okf/plans/3d-quality-tier.md and the notes it cites.
import type { Quality } from "@solidrt/core"
import { MAX_CASCADES } from "./glsl.ts"
import { SHADOW_QUALITY_TIERS } from "./scene-shadows.ts"
import type { ShadowQuality } from "./scene-shadows.ts"

/** Every quality knob of the package, as one row of the table: what each
 * knob defaults to under a tier. The knob's own option (a light's
 * `mapSize`, a scene's `samples`, a texture's `anisotropy`) is the
 * override; what an app does not set follows the row. */
export type QualityRow = {
  /** The soft shadow kernel's tier (SceneOptions.shadows.quality). */
  shadows: ShadowQuality
  /** A casting light's map edge in texels when it sets no `mapSize`. */
  mapSize: number
  /** A directional caster's cascade count when it sets neither
   * `cascades` nor a `camera` box (a boxed light stays one map). */
  cascades: number
  /** The LOD size multiplier (SceneOptions.lodBias). */
  lodBias: number
  /** Whether LOD groups cross-fade through their band (inside a band both
   * levels draw); false switches hard at every threshold. */
  lodFade: boolean
  /** The multisample count of the scene target and of every view that
   * sets none (SceneOptions.samples). */
  samples: 1 | 2 | 4 | 8
  /** The fill-mode pixel cap of a scene or view that sets no `maxPixels`,
   * or null for none (native density). */
  maxPixels: number | null
  /** The anisotropic filtering level a model's textures are created with
   * (clamped to the device's `limits.maxAnisotropy`). */
  anisotropy: number
  /** The mip levels a model's textures skip (their `baseLevel`): 1 samples
   * from the half-size level down, a quarter of the texel bandwidth. */
  mipLimit: number
  /** The face edge of a reflection probe or sky bake that sets no `size`. */
  probeSize: number
}

/** What `quality` takes: a tier name (that tier's row, whatever the app
 * tier is) or a row object, whose missing fields are the app tier's. */
export type QualityInput = Quality | Partial<QualityRow>

// The fill cap of the low tier: the 720p-class pixel count games render
// at on the SM-T500's GPU class (real 3D shading there runs ~26 ms per
// megapixel, so 1 Mpx is the ceiling of a 60 fps frame with room for the
// rest), and what the TV's 1080p caps down to.
const LOW_MAX_PIXELS = 1_000_000

/** The table: per tier, every knob's default. Medium is the row an app
 * that says nothing gets. Spread a row to derive a custom one:
 * `{ ...QUALITY_TIERS.high, samples: 8 }`. */
export const QUALITY_TIERS: Record<Quality, QualityRow> = {
  // The soft kernel costs 50 percent of a 2 Mpx frame on the SM-T500
  // (hard 78 ms, low 117, medium 140) and the TV is below 3 fps at hard
  // already; 512 maps buy ~7 ms on a box light; a fading band draws both
  // levels; MSAA 4x costs ~9 ms at 2 Mpx; the fill cap is the one lever
  // left after the shadow atlas and the hardware compare tap.
  low: {
    shadows: "hard",
    mapSize: 512,
    cascades: 1,
    lodBias: 0.7,
    lodFade: false,
    samples: 1,
    maxPixels: LOW_MAX_PIXELS,
    anisotropy: 1,
    mipLimit: 1,
    probeSize: 64,
  },
  // Unity's Medium: a 5x5 tent, 2 cascades; 4x MSAA is what every example
  // had asked for by hand before the row; the chain's full resolution and
  // native density.
  medium: {
    shadows: "medium",
    mapSize: 1024,
    cascades: 2,
    lodBias: 1,
    lodFade: true,
    samples: 4,
    maxPixels: null,
    anisotropy: 4,
    mipLimit: 0,
    probeSize: 128,
  },
  // Unity's High and Very High: a 7x7 tent, 2048 maps (what spot cones
  // need to stay clean; 15 ms on the tablet, 50 on the TV - not this
  // class's problem), the LOD bias of Very High, the device's full
  // anisotropy, a 256 probe (Godot's desktop reflection size).
  high: {
    shadows: "high",
    mapSize: 2048,
    cascades: 2,
    lodBias: 1.5,
    lodFade: true,
    samples: 4,
    maxPixels: null,
    anisotropy: 16,
    mipLimit: 0,
    probeSize: 256,
  },
}

// The multisample counts a target takes.
const SAMPLE_COUNTS: readonly number[] = [1, 2, 4, 8]

/** Validate a `quality` input where it enters (`what` names the site):
 * a tier name, or a row object whose given fields are each in range.
 * Throws - the dev validation policy. */
export function checkQualityInput(input: unknown, what: string): void {
  if (input === undefined || input === null) return
  if (typeof input === "string") {
    if (!Object.hasOwn(QUALITY_TIERS, input)) {
      throw new Error(what + ': quality must be "low", "medium", "high" or a row object, got ' + input)
    }
    return
  }
  if (typeof input !== "object") throw new Error(what + ": quality must be a tier name or a row object, got " + typeof input)
  let row = input as Partial<QualityRow>
  let bad = (field: string, want: string, got: unknown) => new Error(what + ": quality." + field + " must be " + want + ", got " + got)
  if (row.shadows !== undefined && SHADOW_QUALITY_TIERS[row.shadows] === undefined) {
    throw bad("shadows", '"hard", "low", "medium" or "high"', row.shadows)
  }
  if (row.mapSize !== undefined && !(Number.isInteger(row.mapSize) && row.mapSize >= 1)) throw bad("mapSize", "a positive integer", row.mapSize)
  if (row.cascades !== undefined && !(Number.isInteger(row.cascades) && row.cascades >= 1 && row.cascades <= MAX_CASCADES)) {
    throw bad("cascades", "an integer from 1 to " + MAX_CASCADES, row.cascades)
  }
  if (row.lodBias !== undefined && !(Number.isFinite(row.lodBias) && row.lodBias > 0)) throw bad("lodBias", "a positive number", row.lodBias)
  if (row.lodFade !== undefined && typeof row.lodFade !== "boolean") throw bad("lodFade", "a boolean", row.lodFade)
  if (row.samples !== undefined && !SAMPLE_COUNTS.includes(row.samples)) throw bad("samples", "1, 2, 4 or 8", row.samples)
  if (row.maxPixels !== undefined && row.maxPixels !== null && !(Number.isFinite(row.maxPixels) && row.maxPixels > 0)) {
    throw bad("maxPixels", "a positive number or null", row.maxPixels)
  }
  if (row.anisotropy !== undefined && !(Number.isFinite(row.anisotropy) && row.anisotropy >= 1)) throw bad("anisotropy", "a number >= 1", row.anisotropy)
  if (row.mipLimit !== undefined && !(Number.isInteger(row.mipLimit) && row.mipLimit >= 0)) throw bad("mipLimit", "a non-negative integer", row.mipLimit)
  if (row.probeSize !== undefined && !(Number.isInteger(row.probeSize) && row.probeSize >= 1)) throw bad("probeSize", "a positive integer", row.probeSize)
}

/** The row an input resolves to under the app tier: a name is that
 * tier's row, a row object is the app tier's row under its given fields,
 * nothing is the app tier's row. */
export function resolveQuality(input: QualityInput | null | undefined, tier: Quality): QualityRow {
  if (typeof input === "string") return QUALITY_TIERS[input]
  let base = QUALITY_TIERS[tier]
  if (input === undefined || input === null) return base
  let row = { ...base }
  for (let key of Object.keys(input) as (keyof QualityRow)[]) {
    let value = input[key]
    if (value !== undefined) (row as Record<string, unknown>)[key] = value
  }
  return row
}

/** Whether two rows agree on every knob. */
export function sameQuality(a: QualityRow, b: QualityRow): boolean {
  for (let key of Object.keys(a) as (keyof QualityRow)[]) if (a[key] !== b[key]) return false
  return true
}

/** The texture sampling a tier gives a model's image of `levels` mip
 * levels (1 without a chain): the row's anisotropy, and its mip limit
 * held inside the chain (a 1x1 image has no level to skip). */
export function textureSampling(row: QualityRow, levels: number): { anisotropy: number; baseLevel: number } {
  return { anisotropy: row.anisotropy, baseLevel: Math.min(row.mipLimit, Math.max(0, levels - 1)) }
}
