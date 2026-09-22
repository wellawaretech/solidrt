// A gamepad as input sources, over any accessor of pad snapshots: sticks
// and the dpad as vec2, triggers and shoulders as axes, any button as a
// button, any raw axis by name. Runtime-free (the accessor is injected,
// the public `gamepad` in input-gamepad.ts hands in core's gamepads()),
// so the join logic below runs headless in the checks.
//
// Every read is reactive when the accessor is (gamepads() is a signal),
// so a stick moved while a control's frame loop is off wakes it. Sticks
// pass through a radial dead zone; the screen convention holds (a stick
// pushed up reads y = -1, as the web Gamepad API reports it).
//
// Joining: `createGamepadJoin` makes a device with no slot until a pad no
// other joining device holds presses any button, then keeps that slot -
// Unity's join manager, for a couch game that seats players in the order
// they pick up a pad. A claim lasts until the owning scope is disposed
// (an unmounted pane frees its pad); with no owner, for the session.

import { createEffect, createRoot, createSignal, getOwner, onCleanup, untrack } from "@solidjs/signals"
import type { GamepadState } from "./gamepad"
import type { ActionKind, InputDevice, InputSource } from "./input-map"
import type { Vec2 } from "./input-axes"

// Stick deflection below which a resting stick reads as zero (radial:
// the whole vector, so a stick nudged diagonally also rests).
const STICK_DEADZONE = 0.15
// Deflection (a stick's vector, a trigger or raw axis) that names the
// control a rebind listens for: past a rest twitch, short of full travel.
const LISTEN_THRESHOLD = 0.5

// Source ids (input-id.ts): `gamepad:leftStick`, `gamepad:rightStick`,
// `gamepad:dpad`, `gamepad:triggers`, `gamepad:shoulders`,
// `gamepad:axis:<name>`, `gamepad:button:<name>`. No slot in the id: a
// saved binding restores onto the device the app hands in.
const DEVICE = "gamepad"

export type PadsAccessor = () => (GamepadState | null)[]

export interface GamepadDevice extends InputDevice {
  readonly name: "gamepad"
  /** The slot this device reads: a number for one pad, undefined for every
   * connected pad (`gamepad()`) or for a joining device no pad has claimed
   * yet (reactive: it resolves when one joins). */
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
  /** The source of an id's spec ("button:south", "leftStick", "axis:leftX"). */
  resolve(spec: string): InputSource
  /** For a rebind: `found` gets the next control of `kind` the pad
   * actuates past rest (a button pressed, a trigger or raw axis pulled,
   * a stick pushed or the dpad, as `axis`/`button`/the stick sources);
   * what is held when the listen starts must be released first. Returns
   * the stop. */
  listen(kind: ActionKind, found: (source: InputSource) => void): () => void
}

let deadzone = (x: number, y: number): Vec2 => (Math.hypot(x, y) < STICK_DEADZONE ? [0, 0] : [x, y])

// The sticks as (name, x axis, y axis) for the listener.
const STICKS = [
  ["leftStick", "leftX", "leftY"],
  ["rightStick", "rightX", "rightY"],
] as const
const DPAD_BUTTONS = ["dpadUp", "dpadDown", "dpadLeft", "dpadRight"]

/**
 * The sources over `pads()` (the pads this device reads now, reactive),
 * with `slot()` as the device's slot and `who` as the label prefix.
 */
export function createGamepadDevice(pads: () => GamepadState[], slot: () => number | undefined, who: string): GamepadDevice {
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
    id: `${DEVICE}:${side}Stick`,
    device: DEVICE,
    rate: sumVec2(pad => deadzone(pad.axes[`${side}X`] ?? 0, pad.axes[`${side}Y`] ?? 0)),
  })
  // Same name, same source: unbind by identity works for a resolved id too.
  let axes = new Map<string, InputSource<"axis">>()
  let buttons = new Map<string, InputSource<"button">>()
  let device: GamepadDevice = {
    name: DEVICE,
    get slot() {
      return slot()
    },
    leftStick: stick("left"),
    rightStick: stick("right"),
    dpad: {
      kind: "vec2",
      label: `${who} dpad`,
      id: `${DEVICE}:dpad`,
      device: DEVICE,
      rate: sumVec2(pad => [pressed(pad, "dpadRight") - pressed(pad, "dpadLeft"), pressed(pad, "dpadDown") - pressed(pad, "dpadUp")]),
    },
    triggers: {
      kind: "axis",
      label: `${who} triggers`,
      id: `${DEVICE}:triggers`,
      device: DEVICE,
      rate: sumAxis(pad => (pad.axes.rightTrigger ?? 0) - (pad.axes.leftTrigger ?? 0)),
    },
    shoulders: {
      kind: "axis",
      label: `${who} shoulders`,
      id: `${DEVICE}:shoulders`,
      device: DEVICE,
      rate: sumAxis(pad => pressed(pad, "rightShoulder") - pressed(pad, "leftShoulder")),
    },
    axis(name) {
      if (typeof name !== "string" || name.length === 0) throw new Error(`gamepad.axis: expected an axis name, got ${String(name)}`)
      let source = axes.get(name)
      if (!source) {
        source = { kind: "axis", label: `${who} ${name}`, id: `${DEVICE}:axis:${name}`, device: DEVICE, rate: sumAxis(pad => pad.axes[name] ?? 0) }
        axes.set(name, source)
      }
      return source
    },
    button(name) {
      if (typeof name !== "string" || name.length === 0) throw new Error(`gamepad.button: expected a button name, got ${String(name)}`)
      let source = buttons.get(name)
      if (!source) {
        source = { kind: "button", label: `${who} ${name}`, id: `${DEVICE}:button:${name}`, device: DEVICE, rate: anyButton(name) }
        buttons.set(name, source)
      }
      return source
    },
    resolve(spec) {
      let colon = spec.indexOf(":")
      let head = colon < 0 ? spec : spec.slice(0, colon)
      let rest = colon < 0 ? "" : spec.slice(colon + 1)
      switch (head) {
        case "leftStick":
        case "rightStick":
        case "dpad":
        case "triggers":
        case "shoulders":
          if (rest) break
          return device[head]
        case "axis":
          return device.axis(rest)
        case "button":
          return device.button(rest)
      }
      throw new Error(`gamepad.resolve: unknown source "${spec}" (leftStick, rightStick, dpad, triggers, shoulders, axis:<name>, button:<name>)`)
    },
    listen(kind, found) {
      if (kind !== "button" && kind !== "axis" && kind !== "vec2") throw new Error(`gamepad.listen: expected a kind, got ${String(kind)}`)
      if (typeof found !== "function") throw new Error("gamepad.listen: expects a function")
      // What is actuated as the listen starts, released before it counts.
      let held = new Set<string>()
      let arm = (pads: GamepadState[]) => {
        let now = new Set<string>()
        for (let pad of pads) {
          for (let b of pad.buttons) now.add(b)
          for (let [name, v] of Object.entries(pad.axes)) if (Math.abs(v) >= LISTEN_THRESHOLD) now.add(name)
          for (let [name, x, y] of STICKS) if (Math.hypot(pad.axes[x] ?? 0, pad.axes[y] ?? 0) >= LISTEN_THRESHOLD) now.add(name)
        }
        for (let name of held) if (!now.has(name)) held.delete(name)
        return now
      }
      for (let name of arm(untrack(pads))) held.add(name)
      let fresh = (now: Set<string>, name: string) => now.has(name) && !held.has(name)
      return createRoot(dispose => {
        createEffect(
          () => pads(),
          ps => {
            let now = arm(ps)
            let pick = (): InputSource | undefined => {
              if (kind === "button") {
                for (let pad of ps) for (let b of pad.buttons) if (fresh(now, b)) return device.button(b)
                return
              }
              if (kind === "axis") {
                for (let pad of ps) for (let name of Object.keys(pad.axes)) if (fresh(now, name)) return device.axis(name)
                return
              }
              for (let [name] of STICKS) if (fresh(now, name)) return device[name]
              for (let pad of ps) for (let b of pad.buttons) if (DPAD_BUTTONS.includes(b) && fresh(now, b)) return device.dpad
            }
            let source = pick()
            if (source) found(source)
          },
          { defer: true },
        )
        return dispose
      })
    },
  }
  return device
}

/** One slot, or every connected pad when `slot` is undefined. */
export function createGamepadSlot(read: PadsAccessor, slot: number | undefined): GamepadDevice {
  if (slot !== undefined && !(Number.isInteger(slot) && slot >= 0)) throw new Error(`gamepad: slot must be a non-negative integer, got ${String(slot)}`)
  let pads = (): GamepadState[] => {
    let all = read()
    if (slot === undefined) return all.filter((p): p is GamepadState => p !== null)
    let pad = all[slot]
    return pad ? [pad] : []
  }
  return createGamepadDevice(pads, () => slot, slot === undefined ? "gamepad" : `gamepad ${slot}`)
}

// Slots held by joining devices, shared so a pad joins once.
let claimed = new Set<number>()

/** A device that claims the next unclaimed pad to press any button. */
export function createGamepadJoin(read: PadsAccessor): GamepadDevice {
  // ownedWrite: the claim lands inside an effect, the release in a cleanup.
  let [slot, setSlot] = createSignal<number | undefined>(undefined, { ownedWrite: true })
  let mine: number | undefined
  // A root of its own: the watch outlives the render scope that created
  // the device only when there is none to tie it to.
  let dispose = createRoot(dispose => {
    createEffect(
      () => read(),
      pads => {
        if (mine !== undefined) return
        for (let i = 0; i < pads.length; i++) {
          let pad = pads[i]
          if (!pad || claimed.has(i) || pad.buttons.length === 0) continue
          mine = i
          claimed.add(i)
          setSlot(i)
          return
        }
      },
    )
    return dispose
  })
  if (getOwner()) {
    onCleanup(() => {
      dispose()
      if (mine !== undefined) claimed.delete(mine)
    })
  }
  let pads = (): GamepadState[] => {
    let s = slot()
    if (s === undefined) return []
    let pad = read()[s]
    return pad ? [pad] : []
  }
  return createGamepadDevice(pads, slot, "gamepad (joined)")
}
