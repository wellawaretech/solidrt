import { capabilities, env, createSignal } from "@solidrt/core"
import type { Capabilities } from "@solidrt/core"

// Policies: how components should behave. Derived from capabilities by a
// replaceable resolver, with per-policy application overrides on top.
// Components consume policy.* and never ask what platform they are on.
// Theme answers "how does it look"; policies answer "how does it behave".

export type InteractionPolicy = "touch" | "desktop" | "hybrid"
export type DensityPolicy = "comfortable" | "compact" | "dense"
export type MotionPolicy = "normal" | "reduced" | "none"
export type NavigationPolicy = "bottomTabs" | "rail" | "sidebar"
export type LayoutPolicy = "singlePane" | "twoPane"

export type Policies = {
  interaction: InteractionPolicy
  density: DensityPolicy
  motion: MotionPolicy
  // Whether focused controls draw a visible focus indicator. Derived from
  // keyboard presence; the runtime cannot yet tell keyboard focus from pointer
  // focus (no Tab traversal), so this is per-session, not per-focus-source.
  focusRing: boolean
  // Multiplier on type-scale font sizes (Dynamic Type). Follows the OS
  // preference (env.textScale); override via setPolicy to pin it.
  textScale: number
  // Base weight compensation for light-on-dark text on this display, in
  // steps of 100 (other steps decode as 400): low-DPI rendering thins
  // inverted-polarity glyphs. Applied by typeWeight to light-on-dark runs
  // only (per-run polarity, or the theme's palette polarity as the default),
  // with one extra step for small font sizes; see typography.ts.
  textWeightDelta: number
  // Application policies: recommendations derived from the window size class.
  // The application owns the final decision; accept them by consuming
  // policy.navigation / policy.layout, or override via setPolicy.
  navigation: NavigationPolicy
  layout: LayoutPolicy
}

export type PolicyResolver = (caps: Capabilities) => Policies

// On runtimes with device enumeration, capabilities reflect connected devices
// from startup, so interaction settles immediately (and adapts on hotplug).
// Elsewhere capabilities are inferred from seen input traffic: interaction
// starts "hybrid" and settles as evidence arrives (the first mouse move flips
// a mouse-only session to "desktop").
export function defaultPolicyResolver(caps: Capabilities): Policies {
  let interaction: InteractionPolicy =
    caps.touch && caps.precisePointer
      ? "hybrid"
      : caps.touch
        ? "touch"
        : caps.precisePointer
          ? "desktop"
          : "hybrid"
  return {
    interaction,
    density: interaction === "desktop" ? "compact" : "comfortable",
    motion: "normal",
    focusRing: caps.keyboardNav,
    textScale: env.textScale,
    textWeightDelta: env.displayScale < 1.5 ? 100 : 0,
    navigation:
      caps.windowSizeClass === "expanded" ? "sidebar" : caps.windowSizeClass === "medium" ? "rail" : "bottomTabs",
    layout: caps.windowSizeClass === "expanded" ? "twoPane" : "singlePane",
  }
}

// Boxed: a bare function as the initial signal value would be taken for the
// writable-memo compute form of createSignal.
let [resolverBox, setResolverBox] = createSignal({ resolve: defaultPolicyResolver as PolicyResolver })
let [overrides, setOverrides] = createSignal<Partial<Policies>>({})

// Computed, not stored: recomputes per read, tracked through capabilities/env.
let resolved = () => resolverBox().resolve(capabilities)

/** Current policies, as reactive reads. */
export let policy = {
  get interaction(): InteractionPolicy {
    return overrides().interaction ?? resolved().interaction
  },
  get density(): DensityPolicy {
    return overrides().density ?? resolved().density
  },
  get motion(): MotionPolicy {
    return overrides().motion ?? resolved().motion
  },
  get focusRing(): boolean {
    return overrides().focusRing ?? resolved().focusRing
  },
  get textScale(): number {
    return overrides().textScale ?? resolved().textScale
  },
  get textWeightDelta(): number {
    return overrides().textWeightDelta ?? resolved().textWeightDelta
  },
  get navigation(): NavigationPolicy {
    return overrides().navigation ?? resolved().navigation
  },
  get layout(): LayoutPolicy {
    return overrides().layout ?? resolved().layout
  },
}

/** Replaces how system policies derive from capabilities. */
export function setPolicyResolver(resolve: PolicyResolver) {
  setResolverBox({ resolve })
}

/**
 * Forces individual policies, overriding the resolver:
 * setPolicy({ density: "dense" }). An explicit undefined hands a policy back
 * to the resolver: setPolicy({ density: undefined }).
 */
export function setPolicy(partial: Partial<Policies>) {
  setOverrides((prev) => ({ ...prev, ...partial }))
}

// How density maps to component metrics: a multiplier on control sizes,
// paddings, and hit targets. Comfortable is the components' designed size.
const DENSITY_SCALE: Record<DensityPolicy, number> = { comfortable: 1, compact: 0.85, dense: 0.7 }

/** Reactive density multiplier for control metrics. */
export function densityScale(): number {
  return DENSITY_SCALE[policy.density]
}
