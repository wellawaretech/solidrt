// The standard input wiring for the 2d camera: the action declarations it
// consumes (its axes, by the shared vocabulary) and a preset that binds
// the devices an app hands in - plain data the app could write by hand,
// applied with `input.bind(camera2dBindings({ pointer, gamepad }))` and
// adjusted afterwards with unbind; nothing binds unless the app says so
// (ARCHITECTURE.md). `pointer` is the feed of the view showing the layer
// (`createPointerFeed()`, handed to `<SpriteLayer pointer>` or
// `<View2d pointer>`, or bridged with feedPointer for an imperative view),
// `gamepad` a core gamepad device, `keyboard` the core keyboard device.
//
// Keys and sticks move the CAMERA where a drag moves the content (the
// arrow right scrolls the view right, as every map does), so those
// bindings go through invert() and `pan` keeps its one meaning.

import { invert } from "@solidrt/core/input"
import type { Binding, GamepadDevice, KeyboardDevice, PointerFeed } from "@solidrt/core"

export type CameraDevices = {
  pointer?: PointerFeed
  gamepad?: GamepadDevice
  keyboard?: KeyboardDevice
}

/** The 2d camera's actions: `pan`, `zoom`, `roll`. */
export let camera2dActions = { pan: "vec2", zoom: "axis", roll: "axis" } as const

/**
 * The 2d camera's standard bindings: one finger or a mouse drag pans and
 * so do two fingers, a pinch and the wheel zoom about the pointer, a twist
 * rolls; the left stick scrolls, the triggers zoom, the right stick's x
 * rolls; the arrow keys scroll and minus/equals zoom.
 */
export function camera2dBindings(devices: CameraDevices): Binding[] {
  let out: Binding[] = []
  let { pointer, gamepad, keyboard } = devices
  if (pointer) {
    out.push(
      { action: "pan", source: pointer.drag },
      { action: "pan", source: pointer.pan },
      { action: "zoom", source: pointer.pinch },
      { action: "zoom", source: pointer.wheel },
      { action: "roll", source: pointer.twist },
    )
  }
  if (gamepad) {
    out.push({ action: "pan", source: invert(gamepad.leftStick) }, { action: "zoom", source: gamepad.triggers }, { action: "roll", source: invert(gamepad.axis("rightX")) })
  }
  if (keyboard) out.push({ action: "pan", source: invert(keyboard.arrows) }, { action: "zoom", source: keyboard.axis("Minus", "Equal") })
  return out
}
