import { env } from "./environment"
import { touchFeedbackSupport } from "./haptics"

// Capabilities: what interactions are possible in the current environment.
// Derived from Environment State on every read - computed, never stored - so
// they are reactive wherever env is. Capabilities describe what is possible,
// not how the UI should behave; behavior lives in the policy layer on top.
//
// Device presence wins when the runtime reports it (so unplugging the mouse
// drops hover); the traffic-inferred seen-flags are the fallback for runtimes
// without device enumeration.

export type WindowSizeClass = "compact" | "medium" | "expanded"

export type Capabilities = {
  /** A pointer can rest over content without pressing (mouse/trackpad). */
  hover: boolean
  /** Pixel-precise pointing (mouse/trackpad), as opposed to a finger. */
  precisePointer: boolean
  /** Direct touch input. */
  touch: boolean
  /** Hardware-key navigation (tab/arrow traversal, shortcuts). */
  keyboardNav: boolean
  /** How much horizontal room the window offers (Material breakpoints). */
  windowSizeClass: WindowSizeClass
  /**
   * The device has a haptic actuator, so `touchFeedback` can be felt. A
   * hardware fact, not the user's settings; `touchFeedbackSupport()` has
   * the per-intent detail.
   */
  haptics: boolean
}

// Material 3 width breakpoints, in logical pixels.
const MEDIUM_MIN_WIDTH = 600
const EXPANDED_MIN_WIDTH = 840

export let capabilities: Capabilities = {
  get hover() {
    return env.inputDevices?.mouse ?? env.mouseSeen
  },
  get precisePointer() {
    return env.inputDevices?.mouse ?? env.mouseSeen
  },
  get touch() {
    return env.inputDevices?.touch ?? env.touchSeen
  },
  get keyboardNav() {
    return env.inputDevices?.keyboard ?? env.keyboardSeen
  },
  get windowSizeClass(): WindowSizeClass {
    let w = env.windowSize.width
    return w >= EXPANDED_MIN_WIDTH ? "expanded" : w >= MEDIUM_MIN_WIDTH ? "medium" : "compact"
  },
  // A hardware fact, not Environment State: it never changes, so it is
  // not reactive.
  get haptics() {
    return touchFeedbackSupport().actuator
  },
}
