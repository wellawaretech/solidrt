import { theme, type Theme } from "./theme"
import { densityScale } from "./policy"

// Density-scaled spacing: a theme.spacing token multiplied by the density
// policy's metric scale, rounded to whole pixels. Use it for gaps and paddings
// that should tighten under compact/dense density; read theme.spacing directly
// only for distances that must not move with density. Reactive when called
// inside a tracked scope, like any theme/policy read.
export function space(token: keyof Theme["spacing"]): number {
  return Math.round(theme.spacing[token] * densityScale())
}
