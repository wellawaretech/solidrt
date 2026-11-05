// The cadence hold: when the app cannot make the display's refresh rate,
// each frame is held for the same whole number of refreshes (30 fps steady
// on a 60 Hz panel rather than 40 to 60 uneven), trading frames at the
// boundary for a steady cadence. The runtime chooses by default (held on
// touch and remote clients, not with a mouse); the app chooses over it when
// it knows itself (agents/performance.md, "The cadence hold").
import { createSignal, runWithOwner } from "@solidjs/signals"
import { setCadenceHold as nativeSetCadenceHold, resetCadenceHold as nativeResetCadenceHold } from "sol:app"
import { on } from "sol:events"

/** A cadence-hold policy: off, automatic, or a fixed number of refreshes per frame. */
export type CadenceHoldPolicy = "off" | "auto" | number

/** The policy in force and the hold it applies. */
export interface CadenceHoldState {
  /** The app's choice, the runtime's default, or the measurement override. */
  policy: CadenceHoldPolicy
  /** Refreshes per frame; 1 is no hold. */
  hold: number
}

/**
 * Choose the cadence-hold policy for the app: "off" for interactive
 * gameplay that makes the refresh rate, "auto" (held only on a sustained
 * need) or a fixed number of refreshes for continuous motion, "default" to
 * hand the choice back to the runtime. Applies at once and starts unheld;
 * it survives input device changes and ends with the app. A call at the
 * app's top level runs before the first frame.
 */
export function setCadenceHold(policy: CadenceHoldPolicy | "default"): void {
  let named = policy === "off" || policy === "auto" || policy === "default"
  if (!named && !(Number.isInteger(policy) && (policy as number) >= 1)) {
    throw new Error(`setCadenceHold: expected "off", "auto", "default" or a whole number of refreshes, got ${String(policy)}`)
  }
  nativeSetCadenceHold(policy)
}

/**
 * End the hold the runtime learned, at once, keeping the policy. Call it at
 * a phase start, after the phase has loaded: a hold an earlier, heavier
 * phase raised is not lowered by measurement alone, since the device slows
 * down (clocks, cores) under the hold it measures.
 */
export function resetCadenceHold(): void {
  nativeResetCadenceHold()
}

let cadenceHoldAccessor: (() => CadenceHoldState) | undefined

/** The cadence-hold policy in force and the hold it applies, as a reactive accessor. */
export function cadenceHold(): CadenceHoldState {
  if (!cadenceHoldAccessor) {
    runWithOwner(null, () => {
      let [state, setState] = createSignal<CadenceHoldState>({ policy: "off", hold: 1 })
      // Sticky event: a subscriber after the last change still observes it.
      on("cadenceHold", ({ policy, hold }: CadenceHoldState) => setState({ policy, hold }))
      cadenceHoldAccessor = state
    })
  }
  return cadenceHoldAccessor!()
}
