import { theme, type TextVariant } from "./theme"
import { policy } from "./policy"

// Resolved font props for a type-scale role, with the text-scale policy
// applied: spread onto a <text> or d-text. fontSize carries policy.textScale;
// lineHeight is relative to the size, so it scales implicitly. Reactive when
// called inside a tracked scope, like any theme/policy read.
export function typeStyle(variant: TextVariant) {
  let role = theme.text[variant]
  return {
    fontFamily: theme.text.fontFamily,
    fontSize: role.size * policy.textScale,
    lineHeight: role.lineHeight,
    fontWeight: role.weight,
  }
}
