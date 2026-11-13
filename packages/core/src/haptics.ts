import { feedback as nativeFeedback, stopFeedback as nativeStopFeedback, feedbackSupport as nativeFeedbackSupport } from "flux:haptics"
import type { FeedbackOptions, HapticIntent, HapticLevel, HapticSupport, ImpactWeight } from "flux:haptics"

export type { FeedbackOptions, HapticIntent, HapticLevel, HapticSupport, ImpactWeight }

/**
 * Lets the user feel what just happened: the device's actuator plays the
 * platform's own effect for `intent`. Name what happened, not how it should
 * feel: `"toggleOn"` for a switch that turned on, `"error"` for a rejected
 * input, `"impact"` for a hit in a game (with `weight` and `intensity`).
 * Each platform maps the intent to its native effect, or to the nearest one
 * where it has none.
 *
 * Every call is safe everywhere: where nothing plays (no actuator, a
 * desktop, a TV) it is a silent no-op, so app code needs no platform
 * branch. The user's system settings always win: with touch feedback off,
 * nothing plays. Under `sol test` the call is recorded, not played
 * (`app.requestedHaptics()`).
 *
 * Throws a TypeError on an unknown intent or weight, and a RangeError on
 * an `intensity` outside 0..1.
 *
 * @example
 * touchFeedback("toggleOn")
 * touchFeedback("impact", { weight: "heavy", intensity: 0.8 })
 */
export function touchFeedback(intent: HapticIntent, options?: FeedbackOptions): void {
  nativeFeedback(intent, options)
}

/** Stops a touch feedback effect that still plays. Safe everywhere. */
export function stopTouchFeedback(): void {
  nativeStopFeedback()
}

/**
 * What this device plays: whether it has an actuator, how well each intent
 * plays (`native`, `approximate` or `none`), and whether `impact` honours
 * its intensity. Hardware facts, the same on every call, not the user's
 * settings. Most apps never need it, since every `touchFeedback` call is
 * safe; read it once in an app that designs around it (a game that falls
 * back to audio cues where nothing plays). `capabilities.haptics` is the
 * actuator fact alone.
 */
export function touchFeedbackSupport(): HapticSupport {
  return nativeFeedbackSupport()
}
