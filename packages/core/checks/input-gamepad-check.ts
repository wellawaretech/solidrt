// Checks for the gamepad device (input-gamepad-device.ts) over a signal
// of pad snapshots standing in for core's gamepads(): the slot and
// every-pad devices' sources (sticks with the dead zone, dpad, triggers,
// shoulders, buttons), and the join device (createGamepadJoin: a pad
// joins on its first button press, one device per pad, disposal frees
// the slot). Runtime-free, so it runs headless on flux, bundled from the
// repo root:
//
//   bunx srt bundle -f --stdout packages/core/checks/input-gamepad-check.ts | target/release/flux -
//
// Deterministic. A failure prints FAIL lines and throws at the end.

import { createRoot, createSignal, flush } from "@solidjs/signals"
import { createGamepadJoin, createGamepadSlot } from "../src/input-gamepad-device.ts"
import type { GamepadState } from "../src/gamepad.ts"
import { createInputMap } from "../src/input.ts"

let failures = 0
let fail = (msg: string) => {
  failures++
  console.log(`FAIL ${msg}`)
}
let near = (a: number, b: number, eps = 1e-9) => Math.abs(a - b) <= eps

let pad = (id: number, buttons: string[] = [], axes: Record<string, number> = {}): GamepadState => ({ id, name: `pad ${id}`, buttons, axes, mapped: true })

// ---- Slot and every-pad devices ----
{
  let [pads, setPads] = createSignal<(GamepadState | null)[]>([])
  let p0 = createGamepadSlot(pads, 0)
  let all = createGamepadSlot(pads, undefined)
  if (p0.slot !== 0 || all.slot !== undefined) fail("slot devices report their slot")
  setPads([pad(1, ["south", "dpadLeft"], { leftX: 0.5, leftY: -0.5, rightX: 0.05, rightY: 0.05, leftTrigger: 0.25, rightTrigger: 1 }), pad(2, ["rightShoulder"], { leftX: 0.5, leftY: 0 })])
  flush()
  let left = p0.leftStick.rate!()
  if (!near(left[0], 0.5) || !near(left[1], -0.5)) fail(`left stick reads the pad's axes, got ${left}`)
  let right = p0.rightStick.rate!()
  if (right[0] !== 0 || right[1] !== 0) fail("a stick inside the dead zone reads zero")
  let dpad = p0.dpad.rate!()
  if (dpad[0] !== -1 || dpad[1] !== 0) fail(`dpad left reads [-1, 0], got ${dpad}`)
  if (!near(p0.triggers.rate!(), 0.75)) fail("triggers read right minus left")
  if (p0.shoulders.rate!() !== 0) fail("pad 0 has no shoulder held")
  if (!p0.button("south").rate!()) fail("south reads pressed")
  if (p0.button("north").rate!()) fail("north reads released")
  if (!near(p0.axis("leftTrigger").rate!(), 0.25)) fail("a raw axis by name")
  // Every-pad device: sums.
  let sum = all.leftStick.rate!()
  if (!near(sum[0], 1) || !near(sum[1], -0.5)) fail(`every-pad stick sums, got ${sum}`)
  if (all.shoulders.rate!() !== 1) fail("every-pad shoulders see pad 1's right shoulder")
  if (!all.button("south").rate!()) fail("every-pad button reads any pad")
  // A disconnected pad leaves a null hole.
  setPads([null, pad(2, ["rightShoulder"], { leftX: 0.5, leftY: 0 })])
  flush()
  if (p0.button("south").rate!()) fail("a disconnected slot reads released")
  if (all.leftStick.rate!()[0] !== 0.5) fail("every-pad skips the hole")
  // Through a map: the source is reactive, the action follows.
  let input = createInputMap({ move: "vec2", jump: "button" })
  input.bind("move", p0.leftStick)
  input.bind("jump", all.button("rightShoulder"))
  flush()
  if (!input.pressed("jump")) fail("map reads the pad through the source")
  setPads([pad(1, [], { leftX: 0.3, leftY: 0.4 }), null])
  flush()
  let move = input.value("move")
  if (!near(move[0], 0.3) || !near(move[1], 0.4) || input.pressed("jump")) fail(`map follows the pad snapshot, got ${move} ${input.pressed("jump")}`)
  let bad: unknown = "x"
  try {
    createGamepadSlot(pads, bad as number)
    fail("a bad slot must throw")
  } catch (err) {
    if (!(err instanceof Error)) fail(`bad slot: unexpected ${err}`)
  }
}

// ---- Joining ----
{
  let [pads, setPads] = createSignal<(GamepadState | null)[]>([pad(1), pad(2)])
  // Two panes, each under a scope of its own (the join releases on dispose).
  let a = createRoot(dispose => ({ dev: createGamepadJoin(pads), dispose }))
  let b = createRoot(dispose => ({ dev: createGamepadJoin(pads), dispose }))
  flush()
  if (a.dev.slot !== undefined || b.dev.slot !== undefined) fail("nothing joins before a press")
  if (a.dev.leftStick.rate!()[0] !== 0) fail("an unjoined device reads zero")
  // Pad 1 (slot 1) presses first: it joins the first device, not the second.
  setPads([pad(1), pad(2, ["south"])])
  flush()
  if (a.dev.slot !== 1) fail(`the first press joins the first device, got ${a.dev.slot}`)
  if (b.dev.slot !== undefined) fail("one pad joins one device")
  // The joined device reads its pad and follows it.
  setPads([pad(1), pad(2, ["south"], { leftX: -1, leftY: 0 })])
  flush()
  if (a.dev.leftStick.rate!()[0] !== -1) fail("a joined device reads its pad")
  if (!a.dev.button("south").rate!()) fail("a joined device reads its buttons")
  // A press on the already-joined pad does not seat it again; slot 0 joins b.
  setPads([pad(1, ["start"]), pad(2, ["south"], { leftX: -1, leftY: 0 })])
  flush()
  if (b.dev.slot !== 0) fail(`the next pad joins the next device, got ${b.dev.slot}`)
  if (a.dev.slot !== 1) fail("a joined device keeps its slot")
  // Reactive slot: a memo-style read sees the join.
  let c = createRoot(dispose => ({ dev: createGamepadJoin(pads), dispose }))
  let seen: (number | undefined)[] = []
  seen.push(c.dev.slot)
  setPads([pad(1, ["start"]), pad(2, ["south"]), pad(3, ["east"])])
  flush()
  seen.push(c.dev.slot)
  if (seen[0] !== undefined || seen[1] !== 2) fail(`a third pad joins the third device, got ${seen}`)
  // Disposal frees the slot for a later joiner.
  a.dispose()
  let d = createRoot(dispose => ({ dev: createGamepadJoin(pads), dispose }))
  flush()
  if (d.dev.slot !== 1) fail(`a disposed device's pad rejoins the next device (its button is still held), got ${d.dev.slot}`)
  // Through a map: slot 1 (pad 2) still holds south, so the action reads
  // pressed the moment the device joins, and follows the release.
  let input = createInputMap({ jump: "button" })
  input.bind("jump", d.dev.button("south"))
  flush()
  if (!input.pressed("jump")) fail("the map reads the joined pad")
  setPads([pad(1, ["start"]), pad(2), pad(3, ["east"])])
  flush()
  if (input.pressed("jump")) fail("the map follows the joined pad's release")
  b.dispose()
  c.dispose()
  d.dispose()
}

// ---- Ids, resolve() and listen() ----
{
  let [pads, setPads] = createSignal<(GamepadState | null)[]>([pad(1, ["start"], { leftX: 0.9, leftY: 0 })])
  let dev = createGamepadSlot(pads, 0)
  if (dev.name !== "gamepad" || dev.leftStick.id !== "gamepad:leftStick" || dev.button("south").id !== "gamepad:button:south" || dev.axis("leftX").id !== "gamepad:axis:leftX") fail("gamepad ids")
  if (dev.button("south").device !== "gamepad") fail("gamepad sources carry their device")
  if (dev.resolve("leftStick") !== dev.leftStick || dev.resolve("dpad") !== dev.dpad) fail("resolve hands back the device's own stick sources")
  if (dev.resolve("button:south") !== dev.button("south")) fail("resolve: same name, same button source")
  if (dev.resolve("axis:rightTrigger") !== dev.axis("rightTrigger")) fail("resolve: same name, same axis source")
  let bad = (spec: string) => {
    try {
      dev.resolve(spec)
      fail(`resolve("${spec}") must throw`)
    } catch (err) {
      if (!(err instanceof Error)) fail(`resolve("${spec}"): unexpected ${err}`)
    }
  }
  bad("leftStick:x")
  bad("stick")
  bad("button:")
  // Listening: what is held or pushed at the start (start, the left
  // stick) must be released first; the next fresh control of the kind
  // is found, as the device's source.
  let found: string[] = []
  let stop = dev.listen("button", s => found.push(s.id))
  setPads([pad(1, ["start", "south"], { leftX: 0.9, leftY: 0 })])
  flush()
  if (found.join() !== "gamepad:button:south") fail(`listen finds the fresh button, got ${found}`)
  stop()
  setPads([pad(1, [], { leftX: 0.9, leftY: 0 })])
  flush()
  found = []
  stop = dev.listen("vec2", s => found.push(s.id))
  setPads([pad(1, [], { leftX: 0.95, leftY: 0.1 })])
  flush()
  if (found.length !== 0) fail("a stick pushed at the start does not count until released")
  setPads([pad(1, [], { leftX: 0, leftY: 0 })])
  flush()
  setPads([pad(1, [], { leftX: 0, leftY: -0.8 })])
  flush()
  if (found.join() !== "gamepad:leftStick") fail(`listen finds the stick pushed after release, got ${found}`)
  stop()
  found = []
  stop = dev.listen("vec2", s => found.push(s.id))
  setPads([pad(1, ["dpadLeft"], { leftX: 0, leftY: -0.8 })])
  flush()
  if (found.join() !== "gamepad:dpad") fail(`the dpad is a vec2 for a listen, got ${found}`)
  stop()
  found = []
  stop = dev.listen("axis", s => found.push(s.id))
  setPads([pad(1, ["dpadLeft"], { leftX: 0, leftY: -0.8, rightTrigger: 0.3 })])
  flush()
  if (found.length !== 0) fail("a trigger short of the threshold is not found")
  setPads([pad(1, ["dpadLeft"], { leftX: 0, leftY: -0.8, rightTrigger: 0.7 })])
  flush()
  if (found.join() !== "gamepad:axis:rightTrigger") fail(`listen finds the pulled trigger as a raw axis, got ${found}`)
  stop()
  setPads([pad(1, [], { rightTrigger: 1 })])
  flush()
  if (found.length !== 1) fail("a stopped listen hears nothing")
}

console.log(failures === 0 ? "INPUT-GAMEPAD-OK" : `INPUT-GAMEPAD-FAIL ${failures}`)
if (failures > 0) throw new Error(`${failures} gamepad check(s) failed`)
