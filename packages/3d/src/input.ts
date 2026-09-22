// The standard input wiring for the 3d camera controls: the action
// declarations each control consumes (its axes, by the shared vocabulary)
// and a preset per control that binds the devices an app hands in. A
// preset is plain data - the bindings the app could write by hand -
// applied with `input.bind(orbitBindings({ pointer, gamepad }))` and
// adjusted afterwards with unbind; nothing binds unless the app says so
// (ARCHITECTURE.md).
//
// Devices are the nouns: `pointer` is the feed of the element showing the
// scene (`createPointerFeed()`, handed to `<Scene pointer>`), `gamepad` a
// core gamepad device (`gamepad(0)`, or `gamepad()` for any pad), and
// `keyboard` the core keyboard device, whose key events reach the map
// through `input.handlers` spread on the window.
//
// Keys and sticks move the CAMERA where a drag moves the content (the
// stick right orbits the eye to the right; a drag right spins the scene
// to the right), so the rotate and pan bindings of those devices go
// through invert() and each axis keeps its one meaning. `look` is the
// exception by nature: a drag and a stick both turn the eye.

import { invert } from "@solidrt/core/input"
import type { Binding, GamepadDevice, KeyboardDevice, PointerFeed } from "@solidrt/core"

export type CameraDevices = {
  pointer?: PointerFeed
  gamepad?: GamepadDevice
  keyboard?: KeyboardDevice
}

/** The orbit control's actions: `rotate`, `zoom`, `pan`, `focus` (an
 * axis: a bound double tap nudges it with the tap's focal). */
export let orbitActions = { rotate: "vec2", zoom: "axis", pan: "vec2", focus: "axis" } as const

// The device half orbit and map presets share: the right stick rotates,
// the triggers zoom, the left stick pans; the arrows rotate and
// minus/equals zoom.
function orbitDeviceBindings(devices: CameraDevices, out: Binding[]): Binding[] {
  let { gamepad, keyboard } = devices
  if (gamepad) {
    out.push({ action: "rotate", source: invert(gamepad.rightStick) }, { action: "zoom", source: gamepad.triggers }, { action: "pan", source: invert(gamepad.leftStick) })
  }
  if (keyboard) {
    out.push({ action: "rotate", source: invert(keyboard.arrows) }, { action: "zoom", source: keyboard.axis("Minus", "Equal") })
  }
  return out
}

/**
 * The orbit camera's standard bindings: a drag rotates, a Ctrl-drag or a
 * right-drag pans (the desktop pans, Three's and Blender's; a chorded or
 * right-button drag feeds only `pan`, never `rotate`), a pinch and the
 * wheel zoom, two fingers pan, a double tap focuses; the right stick
 * rotates, the triggers zoom, the left stick pans; the arrow keys rotate
 * and minus/equals zoom.
 */
export function orbitBindings(devices: CameraDevices): Binding[] {
  let out: Binding[] = []
  let { pointer } = devices
  if (pointer) {
    out.push(
      { action: "rotate", source: pointer.drag },
      { action: "pan", source: pointer.drag("Ctrl") },
      { action: "pan", source: pointer.drag("Right") },
      { action: "pan", source: pointer.pan },
      { action: "zoom", source: pointer.pinch },
      { action: "zoom", source: pointer.wheel },
      { action: "focus", source: pointer.doubleTap },
    )
  }
  return orbitDeviceBindings(devices, out)
}

/**
 * The map preset over the same orbit control (Three's MapControls): a
 * drag PANS, a right-drag or a Ctrl-drag rotates, two fingers rotate, a
 * pinch and the wheel zoom, a double tap focuses; the pad and keys as
 * orbitBindings. Pair it with `panPlane: "ground"` on the control so a
 * pan slides over the map, not across the view.
 */
export function mapBindings(devices: CameraDevices): Binding[] {
  let out: Binding[] = []
  let { pointer } = devices
  if (pointer) {
    out.push(
      { action: "pan", source: pointer.drag },
      { action: "rotate", source: pointer.drag("Ctrl") },
      { action: "rotate", source: pointer.drag("Right") },
      { action: "rotate", source: pointer.pan },
      { action: "zoom", source: pointer.pinch },
      { action: "zoom", source: pointer.wheel },
      { action: "focus", source: pointer.doubleTap },
    )
  }
  return orbitDeviceBindings(devices, out)
}

/** The first-person control's actions: `look`, `move`, `rise`, `boost`
 * (an axis, so a button binding reads 1 while held - the control has
 * no button kind). */
export let firstPersonActions = { look: "vec2", move: "vec2", rise: "axis", boost: "axis" } as const

/**
 * The first-person camera's standard bindings: a drag looks around and so
 * does mouse motion while the pointer is locked (the app locks it: see
 * examples/first-person.tsx); the right stick looks, the left stick
 * walks, the shoulders rise and sink in fly mode, the left stick's press
 * boosts (the usual pad sprint); WASD and the arrows walk, Q/E rise and
 * sink, Shift boosts.
 */
export function firstPersonBindings(devices: CameraDevices): Binding[] {
  let out: Binding[] = []
  let { pointer, gamepad, keyboard } = devices
  if (pointer) out.push({ action: "look", source: pointer.drag }, { action: "look", source: pointer.mouseDelta })
  if (gamepad) {
    out.push({ action: "look", source: gamepad.rightStick }, { action: "move", source: gamepad.leftStick }, { action: "rise", source: gamepad.shoulders })
    out.push({ action: "boost", source: gamepad.button("leftStick") })
  }
  if (keyboard) {
    out.push({ action: "move", source: keyboard.wasd }, { action: "move", source: keyboard.arrows }, { action: "rise", source: keyboard.axis("KeyQ", "KeyE") })
    out.push({ action: "boost", source: keyboard.key("ShiftLeft") }, { action: "boost", source: keyboard.key("ShiftRight") })
  }
  return out
}
