// The light model: what the theme's one light (theme.light) does to a face.
// Pure math over colors, no JSX, so Surface, the hand-drawn parts (a slider
// fill, a segmented indicator) and an app's own lit drawing all read the
// same answers. The theme's direction is quantized into an interned
// LightAxis (themeAxis), and every derived paint is interned too, so an
// unchanged result is the same object and its JSX binding skips the write:
// a moving light costs each face a re-derivation only when the axis steps.
import { createLinearGradient, createMemo, createRoot, mixColors, parseColor, runWithOwner, withAlpha } from "@solidrt/core"
import type { Color, Gradient, ShadowProps } from "@solidrt/core"
import { theme } from "./theme"
import type { Light, ShadowTone } from "./theme"

// Key shadow offset per elevation unit, in px, for a light at 45 degrees
// (xy/z = 1); a lower light throws it further, along the light's xy.
const SHADOW_OFFSET = 1
// Key shadow blur per elevation unit, px.
const SHADOW_BLUR = 2.5
// Key shadow alpha at ambient 0, before ShadowTone.strength.
const SHADOW_ALPHA = 0.42
// Contact shadow: the tight occlusion where the face meets the ground,
// barely offset. It stays when a press fades the key shadow out.
const CONTACT_OFFSET = 0.5
const CONTACT_BLUR = 1.5
const CONTACT_BLUR_PER_UNIT = 0.35
const CONTACT_ALPHA = 0.3
// How far ambient light fills shadows and the shaded side in: at ambient 1
// they keep (1 - AMBIENT_FILL) of their strength.
const AMBIENT_FILL = 0.6
// The steepest slant (xy/z) shadows follow, so a grazing light does not
// throw them across the screen.
const MAX_SLANT = 2.5
// Sheen at strength 1: the fraction of the light color mixed into the lit
// side, and of black into the shaded side (oklab).
const SHEEN_LIGHT = 0.2
const SHEEN_SHADE = 0.24
// How far a lit face moves toward base x light color (a multiply, like
// diffuse light on a colored surface) at ambient 0: a white card under a
// warm light reads warm, a blue button under a pink one turns violet.
// Ambient light is neutral, so it dilutes the tint. A white light leaves
// every base as it is, which is why the stock presets look the same lit.
const LIGHT_TINT = 0.3
// Bevel at strength 1: the alpha of the light color on the lit edge and of
// black on the shaded edge.
const BEVEL_LIGHT = 0.55
const BEVEL_SHADE = 0.45
// A light traveling out of the screen (z <= 0) is clamped to this, i.e. to
// grazing, instead of flipping the shadows.
const MIN_Z = 0.05
// The quantization of the light's direction: the gradient axis turns in
// ANGLE_STEPS steps, the slant and the obliquity move in their own; a
// shadow's offset and blur snap to SHADOW_STEP px. Below these steps the
// change is invisible, and above them every face re-derives.
const ANGLE_STEPS = 128
const SLANT_STEPS = 64
const OBLIQUITY_STEPS = 64
const SHADOW_STEP = 0.25

/** The light's direction, reduced to what the paint needs, quantized and interned. */
export type LightAxis = {
  // The light's travel in the screen plane, unit length (down when head-on).
  ax: number
  ay: number
  // Shadow displacement per unit of height: xy/z, clamped.
  slantX: number
  slantY: number
  // 0 for a head-on light (every face lit alike), 1 for a grazing one.
  obliquity: number
}

let clamp = (v: number, lo: number, hi: number) => (v < lo ? lo : v > hi ? hi : v)

// Color math crosses into the runtime (parse, oklab mix), and a moving light
// asks every face for its colors each move. While the light only turns, the
// colors do not change - only the gradient's geometry does - so results are
// cached by their inputs, with the mix amount quantized so a dragged ambient
// cannot grow the cache without bound.
const CACHE_LIMIT = 4096
const QUANTUM = 512
let cache = new Map<string, unknown>()
function cached<T>(key: string, compute: () => T): T {
  let hit = cache.get(key)
  if (hit !== undefined) return hit as T
  if (cache.size >= CACHE_LIMIT) cache.clear()
  let value = compute()
  cache.set(key, value)
  return value
}
let quantize = (t: number) => Math.round(t * QUANTUM) / QUANTUM

function alpha(color: string, a: number): string {
  let q = quantize(a)
  return cached(`a|${color}|${q}`, () => withAlpha(color, q))
}

// mixColors, keeping the base's alpha (a glass tint stays translucent).
function mix(base: string, other: string, t: number): string {
  let q = quantize(t)
  return cached(`m|${base}|${other}|${q}`, () => {
    let a = ((parseColor(base) >>> 0) & 0xff) / 255
    let mixed = mixColors(base, other, q)
    return a < 1 ? withAlpha(mixed, a) : mixed
  })
}

// base x light, per channel in linear light (a colored light on a colored
// surface), back to an sRGB hex.
function multiply(base: string, light: string): string {
  let a = parseColor(base) >>> 0
  let b = parseColor(light) >>> 0
  let hex = "#"
  for (let shift of [24, 16, 8]) {
    let product = toLinear((a >>> shift) & 0xff) * toLinear((b >>> shift) & 0xff)
    hex += toSrgb(product).toString(16).padStart(2, "0")
  }
  return hex
}
function toLinear(channel: number): number {
  let c = channel / 255
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
}
function toSrgb(linear: number): number {
  let c = linear <= 0.0031308 ? linear * 12.92 : 1.055 * linear ** (1 / 2.4) - 0.055
  return Math.round(clamp(c, 0, 1) * 255)
}

type LinearGradient = Extract<Gradient, { __gradient: "linear" }>

/** The light's direction, quantized and interned: equal steps return the same object. */
export function lightAxis(direction: Light["direction"]): LightAxis {
  let [x, y, z] = direction
  let length = Math.hypot(x, y, z) || 1
  let planar = Math.hypot(x, y)
  let zz = Math.max(z, MIN_Z)
  let index = planar > 1e-4 ? Math.round((Math.atan2(y, x) / (Math.PI * 2)) * ANGLE_STEPS) : ANGLE_STEPS / 4
  let slantX = Math.round(clamp(x / zz, -MAX_SLANT, MAX_SLANT) * SLANT_STEPS) / SLANT_STEPS
  let slantY = Math.round(clamp(y / zz, -MAX_SLANT, MAX_SLANT) * SLANT_STEPS) / SLANT_STEPS
  let obliquity = Math.round((z <= 0 ? 1 : planar / length) * OBLIQUITY_STEPS) / OBLIQUITY_STEPS
  return cached(`L|${index}|${slantX}|${slantY}|${obliquity}`, () => {
    let angle = (index / ANGLE_STEPS) * Math.PI * 2
    return { ax: Math.cos(angle), ay: Math.sin(angle), slantX, slantY, obliquity }
  })
}

let axisMemo: (() => LightAxis) | undefined

/**
 * The theme light's axis, one memo for the app's lifetime (a detached root):
 * it re-runs as the light moves but notifies only when the axis steps, so
 * every face reading it recomputes at the step rate, not per write.
 */
export function themeAxis(): LightAxis {
  axisMemo ??= runWithOwner(null, () => createRoot(() => createMemo(() => lightAxis(theme.light.direction))))
  return axisMemo()
}

/**
 * A two-stop gradient from the lit edge to the shaded one, spanning the box
 * (0..1 coordinates; a diagonal light runs corner to corner). `invert`
 * swaps the ends: a pressed or sunken face catches the light on the far
 * side. Interned per (colors, quantized axis).
 */
export function alongLight(axis: LightAxis, lit: string, shaded: string, invert: boolean, mid?: string): Gradient {
  let s = (Math.abs(axis.ax) + Math.abs(axis.ay)) / 2
  let [from, to] = invert ? [shaded, lit] : [lit, shaded]
  let template = cached(`g|${from}|${mid}|${to}`, () =>
    createLinearGradient(0, 0, 0, 1, mid
      ? [{ offset: 0, color: from }, { offset: 0.5, color: mid }, { offset: 1, color: to }]
      : [{ offset: 0, color: from }, { offset: 1, color: to }]) as LinearGradient)
  return cached(`G|${from}|${mid}|${to}|${axis.ax}|${axis.ay}`, () => ({
    ...template,
    x0: 0.5 - axis.ax * s,
    y0: 0.5 - axis.ay * s,
    x1: 0.5 + axis.ax * s,
    y1: 0.5 + axis.ay * s,
  }))
}

let snap = (v: number) => Math.round(v / SHADOW_STEP) * SHADOW_STEP

// Interned, so an unchanged shadow is the same object and skips its write.
function shadow(x: number, y: number, blur: number, color: string): ShadowProps {
  let sx = snap(x)
  let sy = snap(y)
  let sb = snap(blur)
  return cached(`s|${sx}|${sy}|${sb}|${color}`, () => ({ x: sx, y: sy, blur: sb, color }))
}

/** A base color as the key light colors it (see LIGHT_TINT); keeps the base's alpha. A gradient is left as it is. */
export function litFace(base: Color | Gradient, light: Light): Color | Gradient {
  if (typeof base !== "string") return base
  let t = LIGHT_TINT * (1 - light.ambient)
  if (t <= 0) return base
  return mix(base, cached(`x|${base}|${light.color}`, () => multiply(base, light.color)), t)
}

/**
 * A face's fill: the base color as it is under a flat material, else the
 * base tinted by the light and, from the sheen strength and the light's
 * obliquity, a gradient along the light on top. A gradient base (a caller's
 * own) and a transparent base are left alone.
 */
export function sheenFill(
  base: Color | Gradient,
  light: Light,
  axis: LightAxis,
  sheen: number | undefined,
  invert: boolean,
): Color | Gradient {
  if (!sheen || sheen <= 0 || typeof base !== "string" || base === "transparent") return base
  let face = litFace(base, light) as string
  let k = sheen * axis.obliquity
  if (k <= 0) return face
  let lit = mix(face, light.color, SHEEN_LIGHT * k)
  let shaded = mix(face, "#000000", SHEEN_SHADE * k * (1 - light.ambient * AMBIENT_FILL))
  return alongLight(axis, lit, shaded, invert)
}

/** The bevel stroke's gradient, or undefined when the material has none. */
export function bevelStroke(light: Light, axis: LightAxis, bevel: number | undefined, invert: boolean): Gradient | undefined {
  let k = (bevel ?? 0) * axis.obliquity
  if (k <= 0) return undefined
  let lit = alpha(light.color, BEVEL_LIGHT * k)
  let shaded = alpha("#000000", BEVEL_SHADE * k * (1 - light.ambient * AMBIENT_FILL))
  return alongLight(axis, lit, shaded, invert, alpha(light.color, 0))
}

/** The key shadow a face at `height` casts from the light, or undefined at 0. */
export function keyShadow(light: Light, axis: LightAxis, height: number, tone: ShadowTone): ShadowProps | undefined {
  if (height <= 0 || tone.strength <= 0) return undefined
  return shadow(
    height * SHADOW_OFFSET * axis.slantX,
    height * SHADOW_OFFSET * axis.slantY,
    height * SHADOW_BLUR,
    alpha(tone.color, SHADOW_ALPHA * tone.strength * (1 - light.ambient * AMBIENT_FILL)),
  )
}

/** The contact shadow under a face at `height`, or undefined at 0. */
export function contactShadow(light: Light, axis: LightAxis, height: number, tone: ShadowTone): ShadowProps | undefined {
  if (height <= 0 || tone.strength <= 0) return undefined
  return shadow(
    CONTACT_OFFSET * axis.slantX,
    CONTACT_OFFSET * axis.slantY,
    CONTACT_BLUR + height * CONTACT_BLUR_PER_UNIT,
    alpha(tone.color, CONTACT_ALPHA * tone.strength * (1 - light.ambient * AMBIENT_FILL)),
  )
}

/** A shadow at a fraction of its alpha (a pressed face's key shadow), interned like the rest. */
export function fadedShadow(s: ShadowProps, fraction: number): ShadowProps {
  let a = ((parseColor(s.color) >>> 0) & 0xff) / 255
  return shadow(s.x ?? 0, s.y ?? 0, s.blur ?? 0, alpha(s.color, a * fraction))
}

/**
 * Where the light comes from, as a point on the unit disk (a light pad's
 * coordinates): -direction.xy, normalized. The inverse of lightFrom.
 */
export function lightSource(light: Light): [number, number] {
  let [x, y, z] = light.direction
  let length = Math.hypot(x, y, z) || 1
  return [-x / length, -y / length]
}

/** A light coming from point (px, py) of the unit disk, in front of the screen: for `setTheme({ light: { direction: lightFrom(px, py) } })`. */
export function lightFrom(px: number, py: number): Light["direction"] {
  let r = Math.hypot(px, py)
  if (r > 1) {
    px /= r
    py /= r
    r = 1
  }
  return [-px, -py, Math.sqrt(Math.max(MIN_Z * MIN_Z, 1 - r * r))]
}
