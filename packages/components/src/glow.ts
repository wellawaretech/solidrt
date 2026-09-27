import { parseColor, withAlpha } from "@solidrt/core"
import type { Color, Gradient, ShadowProps } from "@solidrt/core"
import { theme } from "./theme"
import type { Glow, MaterialRole } from "./types"

// Alpha of a glow in its part's own fill color: reads as emitted light
// against a dark ground without blurring the part's edge away.
const GLOW_ALPHA = 0.7
// Growth of the glow radius while a pressable face is hovered or pressed.
const GLOW_ACTIVE = 1.35

/** The glow a part draws: the instance or theme.components override when set (null clears), else the role's material glow. */
export function partGlow(override: Glow | null | undefined, role: MaterialRole): Glow | null | undefined {
  return override !== undefined ? override : theme.material[role].glow
}

/**
 * The shadow a part draws as its glow, for a rect's `shadow` prop: zero
 * offset, the glow's radius of blur (grown by GLOW_ACTIVE while `active`),
 * in the glow's color or else the fill at GLOW_ALPHA. A fill with no color
 * to take - transparent, or a gradient - casts nothing rather than a black
 * halo. Draw it on an opaque rect (the part's own fill rect, or one in its
 * color hidden under it), never on a transparent one: a transparent
 * caster's blurred shadow composites in a save layer of its own, which a
 * tiled GPU pays for per part (okf/notes/lit-list-cost.md).
 */
export function glowShadow(glow: Glow | null | undefined, fill: Color | Gradient, active = false): ShadowProps | undefined {
  if (!glow) return undefined
  let color = glow.color
  if (color == null) {
    if (typeof fill !== "string" || ((parseColor(fill) >>> 0) & 0xff) === 0) return undefined
    color = withAlpha(fill, GLOW_ALPHA)
  }
  return { x: 0, y: 0, blur: glow.radius * (active ? GLOW_ACTIVE : 1), color }
}
