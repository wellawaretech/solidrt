// Haptics: touch feedback from the device's actuator, named by intent. The
// app says what happened, not how it should feel: each platform plays its
// own effect for the intent and follows the user's system settings (touch
// feedback off silences it). Every call is accepted everywhere; where
// nothing plays (no actuator, a desktop, a TV) it is a silent no-op, so app
// code needs no platform branch. Listed in `Flux.capabilities` as "haptics"
// only where the device has an actuator. Under `sol test` nothing plays: the
// test host records each call (`app.requestedHaptics()`), so a test engine
// lists "haptics" whatever its platform has. A dev client plays it and
// records it too, for an agent to read back.

declare module "flux:haptics" {
  /**
   * What happened, the name a feedback call takes. Pick by meaning, not by
   * how the effect feels on one device: each platform maps the intent to its
   * own effect, and the nearest one where it has no effect of its own.
   *
   * | intent | meaning |
   * |---|---|
   * | `selection` | moved to another discrete choice (a picker, a slider step) |
   * | `alignment` | snapped to a guide or an edge |
   * | `thresholdOn` | a drag crossed the point where release acts (pull to refresh armed) |
   * | `thresholdOff` | the drag crossed back |
   * | `toggleOn` | a switch turned on |
   * | `toggleOff` | a switch turned off |
   * | `longPress` | a long press triggered an action |
   * | `dragStart` | an item was picked up |
   * | `success` | an action finished |
   * | `error` | an action failed, an input was rejected |
   * | `warning` | an action needs care |
   * | `impact` | a collision, a landing, a physical hit; takes `weight` and `intensity` |
   */
  export type HapticIntent =
    | "selection"
    | "alignment"
    | "thresholdOn"
    | "thresholdOff"
    | "toggleOn"
    | "toggleOff"
    | "longPress"
    | "dragStart"
    | "success"
    | "error"
    | "warning"
    | "impact"

  /** How heavy an `impact` is. */
  export type ImpactWeight = "light" | "medium" | "heavy"

  /**
   * How well the platform plays an intent: `native` (it has its own effect
   * for it), `approximate` (another intent's effect plays for it), `none`
   * (nothing plays).
   */
  export type HapticLevel = "native" | "approximate" | "none"

  /**
   * Options of a feedback call. Both apply to `impact` only; every other
   * intent accepts and ignores them.
   */
  export interface FeedbackOptions {
    /** How heavy the hit is. Default "medium". */
    weight?: ImpactWeight
    /**
     * How strong the hit is, 0..1. Default 1. Played only where
     * `feedbackSupport().impactIntensity` is true; elsewhere the weight's effect
     * plays at its own strength.
     */
    intensity?: number
  }

  /**
   * What this device plays: hardware and build facts, not the user's
   * settings (an intent the user has silenced is still `native`).
   */
  export interface HapticSupport {
    /** The device has a haptic actuator. */
    actuator: boolean
    /** How well each intent plays here. */
    intents: Record<HapticIntent, HapticLevel>
    /** `impact` honours its `intensity` option. */
    impactIntensity: boolean
  }

  /**
   * Plays the platform's effect for `intent`, as the user's system settings
   * allow. Returns at once; a no-op where nothing plays. Under `sol test`
   * the call is recorded, not played (`app.requestedHaptics()`).
   *
   * Throws a TypeError on an unknown intent, a non-object `options` or an
   * unknown weight, and a RangeError on an `intensity` that is not a finite
   * number in 0..1.
   *
   * ```ts
   * feedback("toggleOn")
   * feedback("impact", { weight: "heavy", intensity: 0.8 })
   * ```
   */
  export function feedback(intent: HapticIntent, options?: FeedbackOptions): void
  /** Stops an effect that still plays. A no-op where nothing plays. */
  export function stopFeedback(): void
  /**
   * What this device plays. A frozen object with the same facts on every
   * call: read it once, for an app that designs around it (a game that
   * falls back to audio cues).
   */
  export function feedbackSupport(): HapticSupport
}
