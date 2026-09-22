// Checks for the input map (input-map.ts), the control-side axes contract
// (input-axes.ts) and the keyboard device (input-keyboard.ts): source
// combination and its clamps, the button-on-axis rule, kind mismatches
// throwing, injection by name, the delta channel with its brackets,
// drive() into createAxes, invert()/scale(), the bindings listing, and
// the keyboard composites and modifier specs over synthetic key events,
// and enable/disable contexts. Pure-module input
// only (the `@solidrt/core/input` entry imports no runtime module), so it
// runs headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/core/checks/input-map-check.ts | target/release/flux -
//
// Deterministic. A failure prints FAIL lines and throws at the end, so a
// CI step can gate on the exit code. The gamepad device and the pointer
// feed need the runtime and are exercised live by the camera examples.

import { createSignal, flush } from "@solidjs/signals"
import { chord, createAxes, createInputMap, doubleTap, hold, invert, keyboard, resolveSource, scale, tap } from "../src/input.ts"
import { chordName, mostSpecific, parseModifiers } from "../src/input-chord.ts"
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

// The chord vocabulary both devices share: canonical order and name,
// most specific wins, unknown names throw.
{
  let mods = parseModifiers("chord", "Ctrl+Shift", ["Ctrl", "Shift"])
  if (chordName(mods) !== "Shift+Ctrl") fail(`chord canonical order: ${chordName(mods)}`)
  if (chordName(parseModifiers("chord", "Control+Ctrl", ["Control", "Ctrl"])) !== "Ctrl") fail("chord dedupes Control/Ctrl")
  if (chordName([]) !== "") fail("bare chord name")
  throws('parseModifiers("Hyper")', () => parseModifiers("chord", "Hyper", ["Hyper"]))
  let specs = [{ mods: parseModifiers("chord", "", []) }, { mods: parseModifiers("chord", "Ctrl", ["Ctrl"]) }, { mods: parseModifiers("chord", "Ctrl+Shift", ["Ctrl", "Shift"]) }]
  let ev = (ctrl: boolean, shift: boolean) => ({ shiftKey: shift, ctrlKey: ctrl, altKey: false, metaKey: false })
  let pick = (ctrl: boolean, shift: boolean) => mostSpecific(specs, s => s.mods, ev(ctrl, shift)).map(s => chordName(s.mods))
  if (pick(false, false).join() !== "") fail(`mostSpecific bare: ${pick(false, false)}`)
  if (pick(true, false).join() !== "Ctrl") fail(`mostSpecific Ctrl: ${pick(true, false)}`)
  if (pick(true, true).join() !== "Shift+Ctrl") fail(`mostSpecific Ctrl+Shift: ${pick(true, true)}`)
  if (pick(false, true).join() !== "") fail(`mostSpecific Shift alone falls to bare: ${pick(false, true)}`)
}

// A value source of one kind with a settable value.
function valued<K extends "axis" | "vec2" | "button">(kind: K, label: string, initial: number | Vec2 | boolean) {
  let value = initial
  let source: InputSource<K> = { kind, label, id: `custom:${label}`, rate: () => value as never }
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
    id: "custom:drag",
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

// ---- Modifier specs ----
{
  let input = createInputMap({ cycle: "axis", save: "button", jump: "button" })
  input.bind("cycle", keyboard.axis("Shift+Tab", "Tab"))
  input.bind("save", keyboard.key("Ctrl+KeyS"))
  input.bind("jump", keyboard.key("Space"))
  let h = input.handlers
  h.onKeyDown(key("Tab"))
  flush()
  if (input.value("cycle") !== 1) fail("Tab cycles forward")
  // Shift lands while Tab is held and the repeat carries it: the more
  // specific spec supersedes the bare one instead of cancelling it.
  h.onKeyDown({ ...key("Tab"), shiftKey: true, repeat: true })
  flush()
  if (input.value("cycle") !== -1) fail("Shift+Tab cycles back (most specific spec wins)")
  // Shift let go first: the bare up still releases the modified spec.
  h.onKeyUp(key("Tab"))
  flush()
  if (input.value("cycle") !== 0) fail("a bare Tab up releases Shift+Tab")
  h.onKeyDown(key("KeyS", "s"))
  flush()
  if (input.pressed("save")) fail("S without Ctrl does not save")
  h.onKeyUp(key("KeyS", "s"))
  h.onKeyDown({ ...key("KeyS", "s"), ctrlKey: true })
  flush()
  if (!input.pressed("save")) fail("Ctrl+S saves")
  h.onKeyUp(key("KeyS", "s"))
  flush()
  if (input.pressed("save")) fail("S up releases Ctrl+S")
  // Space by its logical key (a synthetic event without a code).
  h.onKeyDown(key("", " "))
  flush()
  if (!input.pressed("jump")) fail("logical ' ' matches Space")
  h.onKeyUp(key("", " "))
  flush()
  if (input.pressed("jump")) fail("logical ' ' up releases Space")
  throws("unknown modifier", () => keyboard.key("Foo+Tab"))
  throws("modifier without a key", () => keyboard.key("Shift+"))
}

// ---- Contexts: enable/disable by action name ----
{
  let input = createInputMap({ move: "vec2", jump: "button", zoom: "axis" })
  let stick = valued("vec2", "stick", [0.5, 0])
  let btn = valued("button", "btn", true)
  input.bind("move", stick.source)
  input.bind("jump", btn.source)
  let presses = 0
  let releases = 0
  input.onPress("jump", () => presses++)
  input.onRelease("jump", () => releases++)
  flush()
  if (!input.enabled("move") || !input.pressed("jump")) fail("actions start enabled")
  input.disable("move", "jump")
  flush()
  if (!same(input.value("move"), [0, 0])) fail(`a disabled vec2 reads neutral, got ${input.value("move")}`)
  if (input.pressed("jump")) fail("a disabled button reads released")
  if (input.enabled("jump")) fail("enabled() reports the switch")
  if (releases !== 1) fail(`disabling a held button releases it once, got ${releases}`)
  stick.set([0, 1])
  input.enable("move", "jump")
  flush()
  if (!same(input.value("move"), [0, 1])) fail("a source moved while disabled reads its current value on enable")
  if (!input.pressed("jump") || presses !== 1) fail(`a source still held presses on enable, got presses ${presses}`)
  input.enable("jump")
  flush()
  if (presses !== 1) fail("enabling an enabled action is a no-op")
  // The delta channel: a disable closes the open gesture, then drops
  // everything; an end whose begin was never delivered reaches nobody.
  let begins = 0
  let deltas = 0
  let ends = 0
  input.onGesture("zoom", { begin: () => begins++, delta: () => deltas++, end: () => ends++ })
  input.begin("zoom")
  input.nudge("zoom", 1)
  input.disable("zoom")
  if (begins !== 1 || deltas !== 1 || ends !== 1) fail(`disable closes the open gesture, got ${begins} ${deltas} ${ends}`)
  input.nudge("zoom", 1)
  input.begin("zoom")
  input.end("zoom")
  if (begins !== 1 || deltas !== 1 || ends !== 1) fail("a disabled action drops its deltas and brackets")
  input.enable("zoom")
  input.end("zoom")
  if (ends !== 1) fail("an end without a delivered begin is dropped")
  input.nudge("zoom", 2)
  input.begin("zoom")
  input.end("zoom")
  if (deltas !== 2 || begins !== 2 || ends !== 2) fail("enabled again, the channel flows")
  // Through drive(): a disabled action stops driving the control's axes.
  let axes = createAxes<{ move: "vec2" }>({ move: "vec2" }, {})
  input.drive(axes, { move: "move" })
  flush()
  if (!same(axes.rate("move"), [0, 1])) fail("drive reads the enabled action")
  input.disable("move")
  flush()
  if (!same(axes.rate("move"), [0, 0]) || axes.active()) fail("a disabled action reads neutral through drive()")
  throws("enable nothing", () => (input.enable as () => void)())
  throws("disable unknown", () => input.disable("fly" as never))
}

// ---- Source ids, save() and load() ----
{
  if (keyboard.key("Shift+Tab").id !== "keyboard:key:Shift+Tab") fail(`key id: ${keyboard.key("Shift+Tab").id}`)
  if (keyboard.wasd.id !== "keyboard:vec2:KeyW/KeyS/KeyA/KeyD") fail(`wasd id: ${keyboard.wasd.id}`)
  if (keyboard.axis("Minus", "Equal").id !== "keyboard:axis:Minus/Equal") fail("axis id")
  if (invert(keyboard.arrows).id !== "invert(keyboard:vec2:ArrowUp/ArrowDown/ArrowLeft/ArrowRight)") fail(`invert id: ${invert(keyboard.arrows).id}`)
  if (scale(keyboard.arrows, 0.5).id !== "scale(0.5,keyboard:vec2:ArrowUp/ArrowDown/ArrowLeft/ArrowRight)") fail("scale id")
  if (hold(keyboard.key("Space")).id !== "hold(400,keyboard:key:Space)") fail(`hold id: ${hold(keyboard.key("Space")).id}`)
  if (chord(keyboard.key("Shift"), keyboard.key("KeyA")).id !== "chord(keyboard:key:Shift,keyboard:key:KeyA)") fail("chord id")
  let devices = { keyboard }
  let input = createInputMap({ move: "vec2", zoom: "axis", jump: "button", fire: "button" })
  input.bind("move", keyboard.wasd, invert(keyboard.arrows))
  input.bind("zoom", scale(keyboard.axis("Minus", "Equal"), 2))
  input.bind("jump", keyboard.key("Space"))
  let saved = input.save()
  let want = {
    move: ["keyboard:vec2:KeyW/KeyS/KeyA/KeyD", "invert(keyboard:vec2:ArrowUp/ArrowDown/ArrowLeft/ArrowRight)"],
    zoom: ["scale(2,keyboard:axis:Minus/Equal)"],
    jump: ["keyboard:key:Space"],
    fire: [],
  }
  if (JSON.stringify(saved) !== JSON.stringify(want)) fail(`save(): ${JSON.stringify(saved)}`)
  // A round trip through JSON restores working sources.
  let other = createInputMap({ move: "vec2", zoom: "axis", jump: "button", fire: "button" })
  other.bind("fire", keyboard.key("KeyF"))
  other.load(JSON.parse(JSON.stringify(saved)), devices)
  if (JSON.stringify(other.save()) !== JSON.stringify(want)) fail(`load() round trip: ${JSON.stringify(other.save())}`)
  other.handlers.onKeyDown(key("ArrowUp"))
  other.handlers.onKeyDown(key("Equal"))
  flush()
  if (!same(other.value("move"), [0, 1])) fail(`a loaded inverted composite reads inverted, got ${other.value("move")}`)
  if (other.value("zoom") !== 1) fail("a loaded scaled axis reads scaled (clamped)")
  other.handlers.onBlur()
  // Partial load touches only the actions named.
  other.load({ fire: ["keyboard:key:KeyF"] }, devices)
  if (other.bindings("move").length !== 2 || other.bindings("fire")[0]!.source.id !== "keyboard:key:KeyF") fail("a partial load keeps the other actions")
  // Every failure leaves the map untouched.
  throws("load an unknown device", () => other.load({ jump: ["gamepad:button:south"] }, devices))
  throws("load an unknown processor", () => other.load({ jump: ["twist(keyboard:key:Space)"] }, devices))
  throws("load a kind mismatch", () => other.load({ jump: ["keyboard:vec2:KeyW/KeyS/KeyA/KeyD"] }, devices))
  throws("load a bad spec", () => other.load({ move: ["keyboard:vec2:KeyW/KeyS"] }, devices))
  throws("load an unknown action", () => other.load({ fly: ["keyboard:key:Space"] } as never, devices))
  if (other.bindings("jump")[0]!.source.id !== "keyboard:key:Space") fail("a failed load changes nothing")
  throws("resolve without a device", () => resolveSource("keyboard:key:Space", {}))
  throws("resolve a bad number", () => resolveSource("scale(x,keyboard:axis:Minus/Equal)", devices))
}

// A device standing in for a pad or a pointer feed: `listen` reports
// whatever the check pushes, and `resolve` hands back the pushed sources.
function fakeDevice(name: "gamepad" | "pointer") {
  let listeners = new Set<(source: InputSource) => void>()
  let known = new Map<string, InputSource>()
  let make = <K extends "button" | "axis" | "vec2">(kind: K, spec: string) => {
    let source: InputSource<K> = { kind, label: `${name} ${spec}`, id: `${name}:${spec}`, device: name, rate: () => false as never }
    known.set(spec, source)
    return source
  }
  return {
    name,
    make,
    resolve(spec: string) {
      let s = known.get(spec)
      if (!s) throw new Error(`fake ${name}: unknown ${spec}`)
      return s
    },
    listen(_kind: "button" | "axis" | "vec2", found: (source: InputSource) => void) {
      listeners.add(found)
      return () => {
        listeners.delete(found)
      }
    },
    push(source: InputSource) {
      listeners.forEach(l => l(source))
    },
    get listening() {
      return listeners.size
    },
  }
}

// A value source behind a signal: its edges reach the effects an
// interaction, an edge callback or a device watcher run.
function signalled<K extends "axis" | "vec2" | "button">(kind: K, label: string, initial: number | Vec2 | boolean) {
  let [value, set] = createSignal(initial, { ownedWrite: true })
  let source: InputSource<K> = { kind, label, id: `custom:${label}`, rate: () => value() as never }
  return { source, set: (v: number | Vec2 | boolean) => set(v) }
}

let tick = () => new Promise<void>(resolve => setTimeout(resolve, 0))
let sleep = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms))

async function asyncChecks() {
  // ---- rebind(): keys ----
  {
    let input = createInputMap({ jump: "button", move: "vec2", zoom: "axis" })
    input.bind("jump", keyboard.key("Space"))
    input.bind("move", keyboard.wasd, keyboard.arrows)
    input.bind("zoom", keyboard.axis("Minus", "Equal"))
    let h = input.handlers
    let pending = input.rebind("jump", { keyboard })
    throws("a second rebind while one is pending", () => input.rebind("jump", { keyboard }))
    // A bare modifier names nothing; a repeat is not a press; the key
    // that lands is captured, not played.
    h.onKeyDown({ ...key("ShiftLeft", "Shift"), shiftKey: true })
    h.onKeyDown({ ...key("KeyJ", "j"), repeat: true })
    h.onKeyDown(key("Space", " "))
    flush()
    if (input.pressed("jump")) fail("a captured key does not play")
    let bound = await pending
    if (bound.id !== "keyboard:key:Space") fail(`rebind binds the captured key, got ${bound.id}`)
    if (input.bindings("jump").length !== 1) fail("replace (default) swaps the same device's binding")
    h.onKeyUp(key("Space", " "))
    // With modifiers, and without replacing.
    let p2 = input.rebind("jump", { keyboard }, { replace: false })
    h.onKeyDown({ ...key("KeyJ", "j"), ctrlKey: true })
    let b2 = await p2
    if (b2.id !== "keyboard:key:Ctrl+KeyJ") fail(`rebind carries modifiers, got ${b2.id}`)
    if (input.bindings("jump").map(b => b.source.id).join() !== "keyboard:key:Space,keyboard:key:Ctrl+KeyJ") fail("replace: false adds")
    h.onKeyUp(key("KeyJ", "j"))
    // A part of a composite: WASD's up becomes I; the arrows stay.
    let p3 = input.rebind("move", { keyboard }, { part: "up" })
    h.onKeyDown(key("KeyI", "i"))
    let b3 = await p3
    if (b3.id !== "keyboard:vec2:KeyI/KeyS/KeyA/KeyD") fail(`part rebind swaps one key, got ${b3.id}`)
    if (input.bindings("move").map(b => b.source.id).join() !== "keyboard:vec2:ArrowUp/ArrowDown/ArrowLeft/ArrowRight,keyboard:vec2:KeyI/KeyS/KeyA/KeyD") fail(`part rebind keeps the other composite: ${input.bindings("move").map(b => b.source.id)}`)
    h.onKeyUp(key("KeyI", "i"))
    h.onKeyDown(key("KeyI", "i"))
    flush()
    if (!same(input.value("move"), [0, -1])) fail("the rebound composite plays")
    h.onKeyUp(key("KeyI", "i"))
    let p4 = input.rebind("zoom", { keyboard }, { part: "pos" })
    h.onKeyDown(key("KeyX", "x"))
    if ((await p4).id !== "keyboard:axis:Minus/KeyX") fail("axis part rebind")
    h.onKeyUp(key("KeyX", "x"))
    // Without a part, an axis action ignores keys.
    let ctrl = new AbortController()
    let p5 = input.rebind("zoom", { keyboard }, { signal: ctrl.signal })
    h.onKeyDown(key("KeyZ", "z"))
    h.onKeyUp(key("KeyZ", "z"))
    let settled = false
    p5.then(
      () => (settled = true),
      () => (settled = true),
    )
    await tick()
    if (settled) fail("a key without a part does not rebind an axis")
    ctrl.abort()
    let reason: unknown
    await p5.catch(err => (reason = err))
    if (!(reason instanceof Error)) fail(`abort rejects with the reason, got ${String(reason)}`)
    // An aborted signal rejects at once; a bad part throws.
    let aborted = new AbortController()
    aborted.abort()
    let early: unknown
    await input.rebind("jump", { keyboard }, { signal: aborted.signal }).catch(err => (early = err))
    if (!(early instanceof Error)) fail("an already-aborted signal rejects at once")
    throws("part on a button", () => input.rebind("jump", { keyboard }, { part: "up" }))
    throws("wrong part for an axis", () => input.rebind("zoom", { keyboard }, { part: "up" }))
    throws("no devices", () => input.rebind("jump", {}))
    // A part with no composite bound rejects.
    let bare = createInputMap({ move: "vec2" })
    let p6 = bare.rebind("move", { keyboard }, { part: "left" })
    bare.handlers.onKeyDown(key("KeyA", "a"))
    let noComposite: unknown
    await p6.catch(err => (noComposite = err))
    if (!(noComposite instanceof Error)) fail("a part rebind with no composite rejects")
    // The map plays again after a rebind.
    h.onKeyDown(key("Space", " "))
    flush()
    if (!input.pressed("jump")) fail("keys play after a rebind")
    h.onBlur()
  }

  // ---- rebind(): listening devices ----
  {
    let pad = fakeDevice("gamepad")
    let pointer = fakeDevice("pointer")
    let input = createInputMap({ jump: "button", look: "vec2" })
    input.bind("jump", keyboard.key("Space"), pad.make("button", "button:east"))
    let p = input.rebind("jump", { keyboard, gamepad: pad, pointer })
    if (pad.listening !== 1 || pointer.listening !== 1) fail("rebind listens on every device given")
    // A source of the wrong kind is ignored; the right one binds and
    // replaces the pad's binding only.
    pad.push(pad.make("vec2", "leftStick"))
    pad.push(pad.make("button", "button:south"))
    let bound = await p
    if (bound.id !== "gamepad:button:south") fail(`pad rebind, got ${bound.id}`)
    if (input.bindings("jump").map(b => b.source.id).join() !== "keyboard:key:Space,gamepad:button:south") fail(`replace is per device: ${input.bindings("jump").map(b => b.source.id)}`)
    if (pad.listening !== 0 || pointer.listening !== 0) fail("a settled rebind stops listening")
    let p2 = input.rebind("look", { gamepad: pad, pointer })
    pointer.push(pointer.make("vec2", "drag:Ctrl"))
    if ((await p2).id !== "pointer:drag:Ctrl") fail("pointer rebind")
    // A loaded id resolves through the device.
    input.load({ look: ["invert(gamepad:leftStick)"] }, { gamepad: pad })
    if (input.bindings("look")[0]!.source.id !== "invert(gamepad:leftStick)") fail("load through a device's resolve")
  }

  // ---- Interactions: hold, tap, doubleTap, chord ----
  {
    let input = createInputMap({ charge: "button", dash: "button", dodge: "button", both: "button" })
    let a = signalled("button", "a", false)
    let b = signalled("button", "b", false)
    input.bind("charge", hold(a.source, 30))
    input.bind("dash", tap(a.source, 30))
    input.bind("dodge", doubleTap(a.source, 60, 30))
    input.bind("both", chord(a.source, b.source))
    let counts = { charge: 0, dash: 0, dodge: 0, both: 0 }
    for (let name of Object.keys(counts) as (keyof typeof counts)[]) input.onPress(name, () => counts[name]++)
    // A short press: a tap, not a hold.
    a.set(true)
    flush()
    a.set(false)
    flush()
    await tick()
    if (counts.dash !== 1 || counts.charge !== 0) fail(`a short press taps (${counts.dash}) and does not hold (${counts.charge})`)
    if (input.pressed("dash")) fail("a tap releases on its own")
    // A long press: a hold, not a tap.
    a.set(true)
    flush()
    await sleep(50)
    if (!input.pressed("charge")) fail("held past the time, hold reads pressed")
    a.set(false)
    flush()
    await tick()
    if (input.pressed("charge") || counts.dash !== 1) fail("release ends the hold and a long press is not a tap")
    // Two quick taps: a double tap, and two taps. The gap since the
    // taps above is let pass first, so the pair is the one counted.
    await sleep(80)
    for (let i = 0; i < 2; i++) {
      a.set(true)
      flush()
      a.set(false)
      flush()
      await tick()
    }
    if (counts.dodge !== 1) fail(`two quick taps double-tap once, got ${counts.dodge}`)
    if (counts.dash !== 3) fail(`each tap counts, got ${counts.dash}`)
    // Two slow taps: no double tap.
    await sleep(80)
    a.set(true)
    flush()
    a.set(false)
    flush()
    await sleep(80)
    a.set(true)
    flush()
    a.set(false)
    flush()
    await tick()
    if (counts.dodge !== 1) fail("taps too far apart do not double-tap")
    // The chord: both, not either.
    a.set(true)
    flush()
    if (input.pressed("both")) fail("a chord needs every source")
    b.set(true)
    flush()
    if (!input.pressed("both") || counts.both !== 1) fail("a chord presses when the last source lands")
    a.set(false)
    flush()
    if (input.pressed("both")) fail("a chord releases when any source lifts")
    throws("hold an axis", () => hold(valued("axis", "z", 0).source as never))
    throws("hold a negative time", () => hold(a.source, -1))
    throws("chord of one", () => chord(a.source))
  }

  // ---- device(): the last device that moved anything bound ----
  {
    let pad = fakeDevice("gamepad")
    let input = createInputMap({ move: "vec2", jump: "button", zoom: "axis" })
    let stickValue: Vec2 = [0, 0]
    let stick: InputSource<"vec2"> = { kind: "vec2", label: "stick", id: "gamepad:leftStick", device: "gamepad", rate: () => stickValue }
    let [stickVersion, bumpStick] = createSignal(0, { ownedWrite: true })
    let reactiveStick: InputSource<"vec2"> = { ...stick, rate: () => (stickVersion(), stickValue) }
    let custom = signalled("button", "custom", false)
    input.bind("move", keyboard.wasd, reactiveStick)
    input.bind("jump", keyboard.key("Space"), custom.source)
    input.bind("zoom", pad.make("axis", "wheel"))
    flush()
    if (input.device() !== undefined) fail("no device before any input")
    input.handlers.onKeyDown(key("KeyW", "w"))
    flush()
    if (input.device() !== "keyboard") fail(`a key names the keyboard, got ${input.device()}`)
    stickValue = [0.5, 0]
    bumpStick(1)
    flush()
    if (input.device() !== "gamepad") fail(`a stick names the pad, got ${input.device()}`)
    // A source without a device never counts; a delta names its device.
    custom.set(true)
    flush()
    if (input.device() !== "gamepad") fail("a custom source without a device does not switch")
    let feedSink: { delta(v: number, f?: Vec2): void } | null = null
    let wheel: InputSource<"axis"> = {
      kind: "axis",
      label: "wheel",
      id: "pointer:wheel",
      device: "pointer",
      deltas(s) {
        feedSink = s
        return () => (feedSink = null)
      },
    }
    input.bind("zoom", wheel)
    feedSink!.delta(1)
    flush()
    if (input.device() !== "pointer") fail(`a delta names its device, got ${input.device()}`)
    input.handlers.onKeyUp(key("KeyW", "w"))
    input.handlers.onKeyDown(key("KeyW", "w"))
    flush()
    if (input.device() !== "keyboard") fail("back to the keyboard")
    input.handlers.onBlur()
  }
}

asyncChecks().then(
  () => {
    console.log(failures === 0 ? "INPUT-MAP-OK" : `INPUT-MAP-FAIL ${failures}`)
    if (failures > 0) throw new Error(`${failures} input map check(s) failed`)
  },
  err => {
    console.log(`INPUT-MAP-FAIL threw: ${err instanceof Error ? (err.stack ?? err.message) : String(err)}`)
    throw err
  },
)
