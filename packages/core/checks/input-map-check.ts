// Checks for the input map (input-map.ts), the control-side axes contract
// (input-axes.ts) and the keyboard device (input-keyboard.ts): source
// combination and its clamps, the button-on-axis rule, kind mismatches
// throwing, injection by name, the delta channel with its brackets,
// drive() into createAxes, invert()/scale(), the bindings listing, and
// the keyboard composites over synthetic key events. Pure-module input
// only (the `@solidrt/core/input` entry imports no runtime module), so it
// runs headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/core/checks/input-map-check.ts | target/release/flux -
//
// Deterministic. A failure prints FAIL lines and throws at the end, so a
// CI step can gate on the exit code. The gamepad device and the pointer
// feed need the runtime and are exercised live by the camera examples.

import { flush } from "@solidjs/signals"
import { createAxes, createInputMap, invert, keyboard, scale } from "../src/input.ts"
import type { InputSource, Vec2 } from "../src/input.ts"
import type { KeyEvent } from "../src/types"

let failures = 0
let fail = (msg: string) => {
  failures++
  console.log(`FAIL ${msg}`)
}
let near = (a: number, b: number, eps = 1e-9) => Math.abs(a - b) <= eps
let same = (a: Vec2, b: Vec2, eps = 1e-9) => near(a[0], b[0], eps) && near(a[1], b[1], eps)
let throws = (what: string, f: () => void) => {
  try {
    f()
    fail(`${what} must throw`)
  } catch (err) {
    if (!(err instanceof Error)) fail(`${what}: unexpected ${err}`)
  }
}

let key = (code: string, k = code): KeyEvent => ({ key: k, code, repeat: false, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, currentTarget: 0, target: 0, stopPropagation() {} })

// A value source of one kind with a settable value.
function valued<K extends "axis" | "vec2" | "button">(kind: K, label: string, initial: number | Vec2 | boolean) {
  let value = initial
  let source: InputSource<K> = { kind, label, rate: () => value as never }
  return { source, set: (v: number | Vec2 | boolean) => (value = v) }
}

// ---- Combination: sum, then clamp ----
{
  let input = createInputMap({ look: "vec2", zoom: "axis", jump: "button" })
  let a = valued("vec2", "a", [0.6, 0])
  let b = valued("vec2", "b", [0.6, 0.8])
  input.bind("look", a.source, b.source)
  let v = input.value("look")
  if (!near(Math.hypot(v[0], v[1]), 1)) fail(`vec2 sum clamps to unit length, got ${v}`)
  b.set([0, 0])
  flush()
  if (!same(input.value("look"), [0.6, 0])) fail(`vec2 sum below unit passes through, got ${input.value("look")}`)
  let z1 = valued("axis", "z1", 0.7)
  let z2 = valued("axis", "z2", 0.7)
  input.bind("zoom", z1.source, z2.source)
  if (input.value("zoom") !== 1) fail(`axis sum clamps to 1, got ${input.value("zoom")}`)
  z2.set(-0.2)
  flush()
  if (!near(input.value("zoom"), 0.5)) fail(`axis sum below 1 passes through, got ${input.value("zoom")}`)
  // A button on an axis reads 1 while pressed.
  let btn = valued("button", "btn", false)
  input.bind("zoom", btn.source)
  btn.set(true)
  flush()
  if (input.value("zoom") !== 1) fail(`a button on an axis adds 1, got ${input.value("zoom")}`)
  // Buttons: any pressed.
  let j = valued("button", "j", false)
  input.bind("jump", j.source)
  if (input.pressed("jump")) fail("jump reads released")
  j.set(true)
  flush()
  if (!input.pressed("jump")) fail("jump reads pressed")
  // Kind mismatches throw.
  throws("vec2 source on an axis", () => input.bind("zoom", a.source))
  throws("axis source on a vec2", () => input.bind("look", z1.source))
  throws("axis source on a button", () => input.bind("jump", z1.source))
  throws("unknown action", () => input.bind("fly" as never, z1.source))
  // Listing and unbinding.
  if (input.bindings("look").length !== 2) fail(`two look bindings, got ${input.bindings("look").length}`)
  input.unbind("look", a.source)
  if (input.bindings("look").length !== 1 || input.bindings().length !== 5) fail("unbind drops one binding")
}

// ---- Injection by name, edges ----
{
  let input = createInputMap({ move: "vec2", jump: "button" })
  input.set("move", [0, -1])
  flush()
  if (!same(input.value("move"), [0, -1])) fail(`set() holds a rate, got ${input.value("move")}`)
  let presses = 0
  let releases = 0
  input.onPress("jump", () => presses++)
  input.onRelease("jump", () => releases++)
  input.press("jump")
  flush()
  input.release("jump")
  flush()
  input.press("jump")
  flush()
  if (presses !== 2 || releases !== 1) fail(`press/release edges: ${presses} presses, ${releases} releases`)
  throws("set a boolean on a vec2", () => input.set("move", true as never))
}

// ---- The delta channel, drive() into axes, processors ----
{
  let input = createInputMap({ rotate: "vec2", zoom: "axis" })
  let log: string[] = []
  let axes = createAxes(
    { rotate: "vec2", zoom: "axis" },
    {
      onBegin: name => log.push(`begin ${name}`),
      onEnd: name => log.push(`end ${name}`),
      onNudge: (name, delta, focal) => log.push(`nudge ${name} ${JSON.stringify(delta)} ${focal ? JSON.stringify(focal) : "-"}`),
    },
  )
  let stop = input.drive(axes)
  // A delta source: emits into whatever the map binds it to.
  let sink: { begin(): void; delta(v: number | Vec2, f?: Vec2): void; end(): void } | null = null
  let drag: InputSource<"vec2"> = {
    kind: "vec2",
    label: "drag",
    deltas(s) {
      sink = s
      return () => (sink = null)
    },
  }
  input.bind("rotate", invert(drag))
  sink!.begin()
  sink!.delta([0.25, -0.5], [0.1, 0.9])
  sink!.end()
  input.nudge("zoom", 2, [0.5, 0.5])
  let want = ["begin rotate", "nudge rotate [-0.25,0.5] [0.1,0.9]", "end rotate", "nudge zoom 2 [0.5,0.5]"]
  if (log.join("|") !== want.join("|")) fail(`delta channel through drive(): ${log.join(" | ")}`)
  if (axes.inGesture("rotate")) fail("no gesture open after end")
  axes.begin("zoom")
  axes.begin("zoom")
  axes.end("zoom")
  if (!axes.inGesture("zoom")) fail("nested begins count")
  axes.end("zoom")
  if (axes.inGesture("zoom")) fail("balanced ends close the gesture")
  // Rates reach the axes; active() follows.
  if (axes.active()) fail("axes rest with nothing bound")
  let r = valued("axis", "r", 0.5)
  input.bind("zoom", scale(r.source, 0.5))
  flush()
  if (!near(axes.rate("zoom"), 0.25)) fail(`scale() halves the rate, got ${axes.rate("zoom")}`)
  if (!axes.active()) fail("a rate wakes axes.active()")
  stop()
  flush()
  if (axes.active()) fail("disconnecting rests the axes")
  input.unbind("rotate", input.bindings("rotate")[0]!.source)
  if (sink !== null) fail("unbind stops the delta subscription")
  throws("drive with a missing action", () => input.drive(createAxes({ pan: "vec2" })))
  throws("drive with a kind mismatch", () => input.drive(createAxes({ zoom: "vec2" })))
  throws("invert a button", () => invert({ kind: "button", label: "b" } as never))
}

// ---- The keyboard device ----
{
  let input = createInputMap({ move: "vec2", rise: "axis", jump: "button" })
  input.bind("move", keyboard.wasd, keyboard.arrows)
  input.bind("rise", keyboard.axis("KeyQ", "KeyE"))
  input.bind("jump", keyboard.key("Space"))
  let h = input.handlers
  h.onKeyDown(key("KeyW", "w"))
  flush()
  if (!same(input.value("move"), [0, -1])) fail(`W reads [0, -1], got ${input.value("move")}`)
  h.onKeyDown(key("KeyD", "d"))
  flush()
  let v = input.value("move")
  if (!near(Math.hypot(v[0], v[1]), 1) || v[0] <= 0 || v[1] >= 0) fail(`W+D is a unit diagonal, got ${v}`)
  h.onKeyUp(key("KeyW", "w"))
  h.onKeyDown(key("ArrowLeft"))
  flush()
  if (!same(input.value("move"), [0, 0])) fail(`D and ArrowLeft cancel, got ${input.value("move")}`)
  h.onKeyDown(key("KeyE", "e"))
  flush()
  if (input.value("rise") !== 1) fail(`E rises, got ${input.value("rise")}`)
  h.onKeyDown(key("Space", " "))
  flush()
  if (!input.pressed("jump")) fail("Space presses jump")
  // A logical key matches when the code does not (a synthetic event).
  h.onKeyDown(key("", "s"))
  flush()
  if (input.value("move")[1] <= 0) fail("logical 's' matches wasd")
  h.onBlur()
  flush()
  if (!same(input.value("move"), [0, 0]) || input.value("rise") !== 0 || input.pressed("jump")) fail("blur releases every held key")
  throws("empty key", () => keyboard.key(""))
}

console.log(failures === 0 ? "INPUT-MAP-OK" : `INPUT-MAP-FAIL ${failures}`)
if (failures > 0) throw new Error(`${failures} input map check(s) failed`)
