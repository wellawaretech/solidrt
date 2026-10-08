import { warmText } from "@solidrt/core"
import type { MeasureTextOptions } from "@solidrt/core"
import { theme, type TextVariant } from "./theme"
import { policy } from "./policy"

// Resolved font props for a type-scale role, with the text policies applied:
// spread onto a <text> or d-text. fontSize carries policy.textScale
// (lineHeight is relative to the size, so it scales implicitly). Reactive
// when called inside a tracked scope, like any theme/policy read. The weight
// is the role's own: the text engine blends coverage with DirectWrite's
// recipe, so light-on-dark text needs no compensation at any display scale.
export function typeStyle(variant: TextVariant) {
  let role = theme.text[variant]
  return {
    fontFamily: theme.text.fontFamily,
    fontSize: role.size * policy.textScale,
    lineHeight: role.lineHeight,
    fontWeight: role.weight,
  }
}

// Every role of the type scale, plus the mono family at body size for code
// spans.
const TYPE_SCALE_VARIANTS: TextVariant[] = ["caption", "label", "body", "title", "heading"]

// The font styles the theme's type scale draws in, as warmText takes them.
// Reactive like typeStyle: a theme or policy change yields the new set.
export function typeScaleStyles(): MeasureTextOptions[] {
  let styles: MeasureTextOptions[] = []
  for (let variant of TYPE_SCALE_VARIANTS) {
    let style = typeStyle(variant)
    styles.push({ fontFamily: style.fontFamily, fontSize: style.fontSize, fontWeight: style.fontWeight })
  }
  let body = typeStyle("body")
  styles.push({ fontFamily: theme.text.monoFamily, fontSize: body.fontSize, fontWeight: body.fontWeight })
  return styles
}

// Warm the text engine for the theme's type scale (see warmText in
// @solidrt/core): the components' Window does this on mount and on every
// theme change; an app without one calls it at startup.
export function warmTypeScale(): void {
  warmText(typeScaleStyles())
}
