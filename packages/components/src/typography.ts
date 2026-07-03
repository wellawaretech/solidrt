import { brightness } from "@solidrt/core"
import { theme, type TextStyle, type TextVariant } from "./theme"
import { policy } from "./policy"

// Below this effective font size, light-on-dark text on a low-DPI display
// needs an extra compensation step (edge pixels dominate small glyphs).
const SMALL_TEXT = 16

// The rendering polarity of `text` drawn on `fill`, for typeWeight/typeStyle:
// true when the text is the lighter of the two. Returns undefined (= fall
// back to the theme's default polarity) when either side is not a comparable
// color (gradients, "transparent").
export function lightOnDark(text: unknown, fill: unknown): boolean | undefined {
  if (typeof text !== "string" || typeof fill !== "string" || fill === "transparent") return undefined
  return brightness(text) > brightness(fill)
}

// The theme's default polarity, derived from its own palette: light text on
// a dark window background means a dark scheme. Nothing to declare per
// preset, and it cannot disagree with the colors.
function themeOnDark(): boolean {
  return lightOnDark(theme.color.text, theme.color.background) ?? false
}

// A themed font weight with low-DPI rendering compensation applied. The
// renderer (Impeller) rasterizes glyphs unhinted and composites the coverage
// in nonlinear sRGB, which steals stem ink from light-on-dark text only (and
// donates it to dark-on-light); the loss grows as glyphs shrink. Compensated
// text adds policy.textWeightDelta (0 on high-DPI displays) plus one extra
// step under SMALL_TEXT px; dark-on-light text passes through untouched.
// `onDark` is the run's own polarity where the caller knows both colors (use
// the lightOnDark() helper, like Button does for its fills); omitted, it
// defaults to the theme's palette polarity. `size` is the effective
// (post-textScale) font size. Clamped to the 900 ceiling; reactive like any
// theme/policy read.
export function typeWeight(weight: number, size: number, onDark?: boolean): TextStyle["weight"] {
  let delta = (onDark ?? themeOnDark()) ? policy.textWeightDelta : 0
  if (delta > 0 && size < SMALL_TEXT) delta += 100
  return Math.min(900, weight + delta) as TextStyle["weight"]
}

// Resolved font props for a type-scale role, with the text policies applied:
// spread onto a <text> or d-text. fontSize carries policy.textScale
// (lineHeight is relative to the size, so it scales implicitly), fontWeight
// carries the typeWeight compensation (pass `onDark` when the text sits on a
// known fill). Reactive when called inside a tracked scope, like any
// theme/policy read.
export function typeStyle(variant: TextVariant, onDark?: boolean) {
  let role = theme.text[variant]
  let size = role.size * policy.textScale
  return {
    fontFamily: theme.text.fontFamily,
    fontSize: size,
    lineHeight: role.lineHeight,
    fontWeight: typeWeight(role.weight, size, onDark),
  }
}
