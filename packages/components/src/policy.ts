import { capabilities, createSignal } from "@solidrt/core"
import type { Capabilities } from "@solidrt/core"

// Policies: how components should behave. Derived from capabilities by a
// replaceable resolver, with per-policy application overrides on top.
// Components consume policy.* and never ask what platform they are on.
// Theme answers "how does it look"; policies answer "how does it behave".

export type InteractionPolicy = "touch" | "desktop" | "hybrid"
export type DensityPolicy = "comfortable" | "compact" | "dense"
export type MotionPolicy = "normal" | "reduced" | "none"

export type Policies = {
  interaction: InteractionPolicy
  density: DensityPolicy
  motion: MotionPolicy
  // Whether focused controls draw a visible focus indicator. Derived from
  // keyboard presence; the runtime cannot yet tell keyboard focus from pointer
  // focus (no Tab traversal), so this is per-session, not per-focus-source.
  focusRing: boolean
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
