import { createSignal } from "@solidjs/signals"
import { on } from "srt:events"

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
 */
export interface GamepadState {
  /** Runtime instance id: unique per connection, not stable across reconnects. */
  id: number
  name: string
  buttons: string[]
  axes: Record<string, number>
}

let gamepadsAccessor: (() => (GamepadState | null)[]) | undefined

/**
 * Connected gamepads as a reactive accessor. Slots are stable web-style: a
 * pad keeps its index for its whole connection, disconnecting leaves a null
 * hole, and the next connect fills the lowest free slot — so slot index works
 * as a persistent player number. Read inside a tracked scope (JSX, memo,
 * effect, onFrame) to re-run on pad activity.
 */
export function gamepads(): (GamepadState | null)[] {
  if (!gamepadsAccessor) {
    // ownedWrite: the sticky event replays synchronously inside on(), which
    // may run within a tracked scope's first read (see environment.ts).
    let [pads, setPads] = createSignal<(GamepadState | null)[]>([], { ownedWrite: true })
    on("gamepads", (e: { pads?: (GamepadState | null)[] }) => setPads(e.pads ?? []))
    gamepadsAccessor = pads
  }
  return gamepadsAccessor()
}
