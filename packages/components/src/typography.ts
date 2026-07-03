import { theme, type TextStyle, type TextVariant } from "./theme"
import { policy } from "./policy"

// A themed font weight with the low-DPI compensation policy applied
// (policy.textWeightDelta), clamped to the 900 ceiling. Reactive like any
// policy read.
export function typeWeight(weight: number): TextStyle["weight"] {
  return Math.min(900, weight + policy.textWeightDelta) as TextStyle["weight"]
}

// Resolved font props for a type-scale role, with the text policies applied:
// spread onto a <text> or d-text. fontSize carries policy.textScale
// (lineHeight is relative to the size, so it scales implicitly), fontWeight
// carries policy.textWeightDelta. Reactive when called inside a tracked
// scope, like any theme/policy read.
export function typeStyle(variant: TextVariant) {
  let role = theme.text[variant]
  return {
    fontFamily: theme.text.fontFamily,
    fontSize: role.size * policy.textScale,
    lineHeight: role.lineHeight,
    fontWeight: typeWeight(role.weight),
  }
}
