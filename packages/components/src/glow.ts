import { parseColor, withAlpha } from "@solidrt/core"
import type { Color, Gradient, ShadowProps } from "@solidrt/core"
import type { Glow } from "./types"

// Alpha of a glow in its part's own fill color: reads as emitted light
// against a dark ground without blurring the part's edge away.
const GLOW_ALPHA = 0.7
// Growth of the glow radius while a pressable face is hovered or pressed.
const GLOW_ACTIVE = 1.35

/** The glow a part draws: the instance or theme.components override when set (null clears), else the theme's role glow. */
export function partGlow(override: Glow | null | undefined, role: Glow | undefined): Glow | null | undefined {
  return override !== undefined ? override : role
}

/**
 * The shadow a part draws as its glow, for a rect's `shadow` prop: zero
 * offset, the glow's radius of blur (grown by GLOW_ACTIVE while `active`),
 * in the glow's color or else the fill at GLOW_ALPHA. A fill with no color
 * to take - transparent, or a gradient - casts nothing rather than a black
 * halo. Draw it on the part's own opaque fill rect, never on a transparent
 * helper rect: a shadow cast by a transparent fill inside an opacity group
 * (a press fade, a popup fade) loses the group's opacity and logs an
 * Impeller validation error every frame.
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
