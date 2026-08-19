import { createContext, useContext } from "@solidrt/core"
import { policy, type DensityPolicy } from "./policy"

// Subtree density: a region (a toolbar, a table, a sidebar) tightens its
// controls wholesale instead of per-child props. The context holds an
// accessor so a reactive `value` stays live for consumers.
let DensityContext = createContext<() => DensityPolicy | undefined>(() => undefined)

export interface DensityProps {
  value: DensityPolicy
  children?: any
}

/**
 * Overrides the density policy for its subtree: every density-scaled metric
 * below - `space()`, control sizes, `Item` row paddings - resolves this value
 * instead of the global `policy.density`. Nests; the nearest wins.
 *
 *   <Density value="compact">
 *     <Item label="..." />   // compact rows, no per-item props
 *   </Density>
 */
export function Density(props: DensityProps) {
  return <DensityContext value={() => props.value}>{props.children}</DensityContext>
}

// How density maps to component metrics: a multiplier on control sizes,
// paddings, and hit targets. Comfortable is the components' designed size.
const DENSITY_SCALE: Record<DensityPolicy, number> = { comfortable: 1, compact: 0.85, dense: 0.7 }

/**
 * Reactive density multiplier for control metrics: the nearest <Density>
 * above the calling scope, falling back to the global `policy.density`.
 * Call it during component setup or inside JSX/thunks (an owner is needed to
 * see a <Density> region; without one it reads the global policy).
 */
export function densityScale(): number {
  return DENSITY_SCALE[useContext(DensityContext)() ?? policy.density]
}
