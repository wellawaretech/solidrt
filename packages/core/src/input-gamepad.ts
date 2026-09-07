// A gamepad as input sources over core's gamepads() snapshot: sticks and
// the dpad as vec2, triggers and shoulders as axes, any button as a
// button, any raw axis by name. `gamepad(slot)` reads one pad (slots are
// stable per connection, so slot 0 is player one for the session);
// `gamepad()` sums every connected pad, the right default for a
// single-player app that does not care which pad is picked up.
//
// Every read is reactive (gamepads() is a signal), so a stick moved while
// a control's frame loop is off wakes it. Sticks pass through a radial
// dead zone; the screen convention holds (a stick pushed up reads y = -1,
// as the web Gamepad API reports it).

import { gamepads } from "./gamepad"
import type { GamepadState } from "./gamepad"
import type { InputSource } from "./input-map"
import type { Vec2 } from "./input-axes"

// Stick deflection below which a resting stick reads as zero (radial:
// the whole vector, so a stick nudged diagonally also rests).
const STICK_DEADZONE = 0.15

export interface GamepadDevice {
  /** The pads this device reads: one slot, or every connected pad. */
  readonly slot: number | undefined
  leftStick: InputSource<"vec2">
  rightStick: InputSource<"vec2">
  /** The dpad as a digital vec2 (each component -1, 0 or 1). */
  dpad: InputSource<"vec2">
  /** Right trigger minus left trigger, -1..1. */
  triggers: InputSource<"axis">
  /** Right shoulder minus left shoulder, -1, 0 or 1. */
  shoulders: InputSource<"axis">
  /** A raw axis by SDL name ("leftX", "rightY", "leftTrigger", ...). */
  axis(name: string): InputSource<"axis">
  /** A button by SDL positional name ("south", "start", "leftShoulder", ...). */
  button(name: string): InputSource<"button">
}

let deadzone = (x: number, y: number): Vec2 => (Math.hypot(x, y) < STICK_DEADZONE ? [0, 0] : [x, y])

export function gamepad(slot?: number): GamepadDevice {
  if (slot !== undefined && !(Number.isInteger(slot) && slot >= 0)) throw new Error(`gamepad: slot must be a non-negative integer, got ${String(slot)}`)
  let who = slot === undefined ? "gamepad" : `gamepad ${slot}`
  let pads = (): GamepadState[] => {
    let all = gamepads()
    if (slot === undefined) return all.filter((p): p is GamepadState => p !== null)
    let pad = all[slot]
    return pad ? [pad] : []
  }
  let sumAxis = (read: (pad: GamepadState) => number) => (): number => {
    let sum = 0
    for (let pad of pads()) sum += read(pad)
    return sum
  }
  let sumVec2 = (read: (pad: GamepadState) => Vec2) => (): Vec2 => {
    let x = 0
    let y = 0
    for (let pad of pads()) {
      let v = read(pad)
      x += v[0]
      y += v[1]
    }
    return [x, y]
  }
  let anyButton = (name: string) => (): boolean => pads().some(pad => pad.buttons.includes(name))
  let pressed = (pad: GamepadState, name: string) => (pad.buttons.includes(name) ? 1 : 0)
  let stick = (side: "left" | "right"): InputSource<"vec2"> => ({
    kind: "vec2",
    label: `${who} ${side} stick`,
    rate: sumVec2(pad => deadzone(pad.axes[`${side}X`] ?? 0, pad.axes[`${side}Y`] ?? 0)),
  })
  return {
    slot,
    leftStick: stick("left"),
    rightStick: stick("right"),
    dpad: {
      kind: "vec2",
      label: `${who} dpad`,
      rate: sumVec2(pad => [pressed(pad, "dpadRight") - pressed(pad, "dpadLeft"), pressed(pad, "dpadDown") - pressed(pad, "dpadUp")]),
    },
    triggers: {
      kind: "axis",
      label: `${who} triggers`,
      rate: sumAxis(pad => (pad.axes.rightTrigger ?? 0) - (pad.axes.leftTrigger ?? 0)),
    },
    shoulders: {
      kind: "axis",
      label: `${who} shoulders`,
      rate: sumAxis(pad => pressed(pad, "rightShoulder") - pressed(pad, "leftShoulder")),
    },
    axis(name) {
      if (typeof name !== "string" || name.length === 0) throw new Error(`gamepad.axis: expected an axis name, got ${String(name)}`)
      return { kind: "axis", label: `${who} ${name}`, rate: sumAxis(pad => pad.axes[name] ?? 0) }
    },
    button(name) {
      if (typeof name !== "string" || name.length === 0) throw new Error(`gamepad.button: expected a button name, got ${String(name)}`)
      return { kind: "button", label: `${who} ${name}`, rate: anyButton(name) }
    },
  }
}
