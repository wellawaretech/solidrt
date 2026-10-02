import { createSignal, runWithOwner } from "@solidjs/signals"
import { on } from "sol:events"

// Gamepad State: a reactive mirror of the runtime's sticky "gamepads" event.
//
// The runtime coalesces pad activity to at most one snapshot per main-loop
// iteration and replays the latest one on subscribe, so the first read
// already sees any connected pads. Runtimes without gamepad support never
// emit the event and the accessor stays [].

/**
 * One connected gamepad's current state. `buttons` holds the names of the
 * currently-pressed buttons, using SDL3's positional names ("south", "east",
 * "west", "north", "dpadUp", "dpadDown", "dpadLeft", "dpadRight", "start",
 * "back", "guide", "leftShoulder", "rightShoulder", "leftStick",
 * "rightStick"). `axes` has sticks ("leftX", "leftY", "rightX", "rightY") in
 * -1..1 and triggers ("leftTrigger", "rightTrigger") in 0..1.
 *
 * The snapshot is a faithful report. Note that pressing "back" (select) on a
 * mapped pad ALSO emits the `back` event (see onBack) - it is the pad-side
 * sibling of Android's system back, the runtime's exit-to-player gesture.
 * Apps that bind "back" for their own controls should preventDefault that
 * event.
 */
export interface GamepadState {
  /** Runtime instance id: unique per connection, not stable across reconnects. */
  id: number
  name: string
  buttons: string[]
  axes: Record<string, number>
  /**
   * True when the device has an SDL controller-database mapping, so button
   * and axis names reflect verified physical positions. False for raw HID
   * joysticks: they still report, but names are assigned positionally in W3C
   * standard-mapping order (button 0 is "south", ..., overflow "button17"...;
   * axes 0-3 are "leftX"..."rightY", overflow "axis4"...), a d-pad hat folds
   * into the "dpad*" names, and analog triggers, if any, appear wherever the
   * device puts them (e.g. as *button* names "leftTrigger"/"rightTrigger" at
   * indices 6/7) rather than as the trigger axes mapped pads have.
   */
  mapped: boolean
}

let gamepadsAccessor: (() => (GamepadState | null)[]) | undefined

// When each pad button last changed: per slot, button name to the
// `timeStamp` of the "gamepads" event that pressed or released it. Kept
// here, where every snapshot passes once, because a reader of the
// accessor sees only the latest one.
let buttonTimes: Map<string, number>[] = []
let lastPads: (GamepadState | null)[] = []

function recordButtonTimes(pads: (GamepadState | null)[], at: number): void {
  for (let slot = 0; slot < Math.max(pads.length, lastPads.length); slot++) {
    let before = lastPads[slot]?.buttons ?? []
    let after = pads[slot]?.buttons ?? []
    let times = (buttonTimes[slot] ??= new Map())
    for (let name of after) if (!before.includes(name)) times.set(name, at)
    for (let name of before) if (!after.includes(name)) times.set(name, at)
  }
  lastPads = pads
}

/** When the button `name` of the pad in `slot` was last pressed or
 * released, as the `timeStamp` (ms, the clock of `PointerEvent.timeStamp`)
 * of the pad snapshot that carried the change; null when it never
 * changed. Needs `gamepads()` to have been read: the snapshots are
 * watched from its first read on. */
export function gamepadButtonChangedAt(slot: number, name: string): number | null {
  return buttonTimes[slot]?.get(name) ?? null
}

/**
 * Connected gamepads as a reactive accessor. Slots are stable web-style: a
 * pad keeps its index for its whole connection, disconnecting leaves a null
 * hole, and the next connect fills the lowest free slot - so slot index works
 * as a persistent player number. Read inside a tracked scope (JSX, memo,
 * effect, onFrame) to re-run on pad activity.
 */
export function gamepads(): (GamepadState | null)[] {
  if (!gamepadsAccessor) {
    // Under no owner: the sticky event replays synchronously inside on(),
    // and the first read is usually a tracked scope (see environment.ts).
    runWithOwner(null, () => {
      let [pads, setPads] = createSignal<(GamepadState | null)[]>([])
      on("gamepads", (e: { pads?: (GamepadState | null)[]; timeStamp: number }) => {
        let next = e.pads ?? []
        recordButtonTimes(next, e.timeStamp)
        setPads(next)
      })
      gamepadsAccessor = pads
    })
  }
  return gamepadsAccessor!()
}
