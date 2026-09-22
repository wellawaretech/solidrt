// Source ids: the serializable half of a binding. A label is what a
// settings screen prints ("gamepad 0 south"); an id is what it saves and
// what a saved file restores from, so it names the source without the
// device instance: `gamepad:button:south` restores onto whichever pad
// device the app hands in, and player 2's file lands on player 2's pad.
//
// The grammar is one line: `<device>:<spec>` for a device's source, the
// spec being the device's own vocabulary (input-keyboard.ts,
// input-gamepad-device.ts, input-pointer.ts each document theirs), and
// `<processor>(<args>)` around it for a processed one, args separated
// by commas with ids nested as they are:
//
//   keyboard:key:Shift+Tab      keyboard:axis:Minus/Equal
//   keyboard:vec2:KeyW/KeyS/KeyA/KeyD
//   gamepad:leftStick           gamepad:button:south       gamepad:axis:leftTrigger
//   pointer:drag                pointer:drag:Ctrl+Right    pointer:wheel
//   invert(gamepad:leftStick)   scale(0.5,pointer:wheel)   hold(400,keyboard:key:Space)
//   chord(keyboard:key:Shift,keyboard:key:KeyA)
//
// A custom source names itself under a prefix of its own; load() resolves
// only the devices it is given and throws on anything else, so a file
// from another version of the app fails loudly, not silently half-bound.

import type { InputDeviceSet, InputSource } from "./input-map"
import { chord, doubleTap, hold, invert, scale, tap } from "./input-processors"

// Split processor arguments at top-level commas (nested ids keep theirs).
let splitArgs = (text: string): string[] => {
  let out: string[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i < text.length; i++) {
    let c = text[i]
    if (c === "(") depth++
    else if (c === ")") depth--
    else if (c === "," && depth === 0) {
      out.push(text.slice(start, i))
      start = i + 1
    }
  }
  out.push(text.slice(start))
  return out
}

let number = (id: string, text: string): number => {
  let n = Number(text)
  if (text.trim() === "" || !Number.isFinite(n)) throw new Error(`resolveSource: "${id}" has a non-numeric argument "${text}"`)
  return n
}

/** The source an id names, built from `devices`; throws when the id names
 * a device not given, an unknown processor, or a spec the device rejects. */
export function resolveSource(id: string, devices: InputDeviceSet): InputSource {
  if (typeof id !== "string" || id.length === 0) throw new Error(`resolveSource: expected an id, got ${String(id)}`)
  let processed = /^([A-Za-z]+)\((.*)\)$/.exec(id)
  if (processed) {
    let [, name, inner] = processed
    let args = splitArgs(inner!)
    let axis = (i: number) => resolveSource(args[i]!, devices) as InputSource<"axis" | "vec2">
    let button = (i: number) => resolveSource(args[i]!, devices) as InputSource<"button">
    let arity = (n: number) => {
      if (args.length !== n) throw new Error(`resolveSource: ${name}() takes ${n} argument(s), "${id}" gives ${args.length}`)
    }
    switch (name) {
      case "invert":
        arity(1)
        return invert(axis(0))
      case "scale":
        arity(2)
        return scale(axis(1), number(id, args[0]!))
      case "hold":
        arity(2)
        return hold(button(1), number(id, args[0]!))
      case "tap":
        arity(2)
        return tap(button(1), number(id, args[0]!))
      case "doubleTap":
        arity(3)
        return doubleTap(button(2), number(id, args[0]!), number(id, args[1]!))
      case "chord":
        return chord(...args.map((_, i) => button(i)))
      default:
        throw new Error(`resolveSource: unknown processor "${name}" in "${id}"`)
    }
  }
  let colon = id.indexOf(":")
  if (colon <= 0) throw new Error(`resolveSource: "${id}" names no device (expected "<device>:<spec>")`)
  let name = id.slice(0, colon) as keyof InputDeviceSet
  let device = devices[name]
  if (!device) throw new Error(`resolveSource: no "${name}" device given for "${id}" (given: ${Object.keys(devices).join(", ") || "none"})`)
  return device.resolve(id.slice(colon + 1))
}
