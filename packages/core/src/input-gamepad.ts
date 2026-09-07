// The gamepad device over core's gamepads() snapshot: `gamepad(slot)`
// reads one pad (slots are stable per connection, so slot 0 is player
// one for the session); `gamepad()` sums every connected pad, the right
// default for a single-player app that does not care which pad is picked
// up; `gamepad.next()` claims the next pad that presses any button, for
// split screen that seats players in pick-up order. The sources and the
// join logic live in input-gamepad-device.ts, runtime-free.

import { gamepads } from "./gamepad"
import { createGamepadJoin, createGamepadSlot } from "./input-gamepad-device"
import type { GamepadDevice } from "./input-gamepad-device"

export type { GamepadDevice } from "./input-gamepad-device"

export function gamepad(slot?: number): GamepadDevice {
  return createGamepadSlot(gamepads, slot)
}

/**
 * A device with no pad until one no other joining device holds presses
 * any button, then that pad for good (`slot` resolves, reactively). The
 * claim is released when the creating scope is disposed.
 */
gamepad.next = (): GamepadDevice => createGamepadJoin(gamepads)
