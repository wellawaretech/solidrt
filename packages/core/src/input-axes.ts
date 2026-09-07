// The control side of the input abstraction (ARCHITECTURE.md: controls
// through an abstraction, never direct event handling). A control - a
// camera controller, a character controller - declares the axes it
// consumes and receives input only through them, in device-free units:
//
// - rates: -1..1 values sampled at update time (a held key, a deflected
//   stick, a value an app set), which the control integrates over dt at
//   its own speeds. Sources are added, never set: several may feed one
//   axis (keys and a stick), and the control sums and clamps them, so
//   removing one never has to know what the others hold.
// - nudges: immediate deltas (a drag, a wheel notch, a pinch) in the
//   vocabulary's units - view heights of travel, octaves of zoom, turns -
//   applied at once, with the focal point (a fraction of the element)
//   when the gesture has one. begin/end bracket a gesture, so a control
//   can tell a finger's tracking (apply exactly) from an impulse (a wheel
//   notch, unbracketed: ease it), stop a glide when a finger lands, and
//   fling when it lifts.
//
// The values use the screen convention throughout: x right, y down, the
// way pointer deltas and the web Gamepad API report (a stick pushed up
// reads y = -1). A control that means "forward" by a vec2 reads -y.
//
// `active()` is the frame-loop gate: reactive, true while any rate source
// reads non-zero, so a control's loop runs only while something drives it.
// A rate source must therefore read reactive state (a signal, gamepads());
// a source over plain variables never wakes the loop - the caller runs
// update(dt) itself then.
//
// An input map drives an Axes object by action name (InputMap.drive); an
// app can also add its own sources and nudges directly.

import { createMemo, createSignal, untrack } from "@solidjs/signals"

export type AxisKind = "axis" | "vec2"
export type Vec2 = [number, number]
export type AxisValue<K extends AxisKind> = K extends "axis" ? number : Vec2

export type AxesDecl = Record<string, AxisKind>

export interface AxesHooks<A extends AxesDecl> {
  /** An immediate delta on `name`: view heights for a vec2, the axis's
   * unit (octaves, turns) for an axis; `focal` in 0..1 of the element
   * when the gesture has one (a pinch, a wheel). */
  onNudge?: <N extends keyof A & string>(name: N, delta: AxisValue<A[N]>, focal: Vec2 | undefined) => void
  /** A gesture on `name` started (a finger landed) / ended (lifted). */
  onBegin?: (name: keyof A & string) => void
  onEnd?: (name: keyof A & string) => void
}

export interface Axes<A extends AxesDecl> {
  /** The declared axes, name to kind - what a map's drive() reads. */
  readonly kinds: A
  /** Add a rate source (reactive read, -1..1 per component); returns the
   * remover. Several sources sum, then clamp (an axis to -1..1, a vec2 to
   * unit length). */
  add<N extends keyof A & string>(name: N, rate: () => AxisValue<A[N]>): () => void
  /** The combined rate of `name` now (reactive read). */
  rate<N extends keyof A & string>(name: N): AxisValue<A[N]>
  /** Reactive: whether any rate reads non-zero - the frame-loop gate. */
  active(): boolean
  /** Apply an immediate delta (see AxesHooks.onNudge). */
  nudge<N extends keyof A & string>(name: N, delta: AxisValue<A[N]>, focal?: Vec2): void
  /** Bracket a gesture on `name`; nested begins count. */
  begin(name: keyof A & string): void
  end(name: keyof A & string): void
  /** Whether a gesture on `name` is open (between begin and end). */
  inGesture(name: keyof A & string): boolean
}

let clampNum = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

/** Sum rate values of one kind and clamp: an axis to -1..1, a vec2 to unit length. */
export function combineRates(kind: AxisKind, values: (number | Vec2)[]): number | Vec2 {
  if (kind === "axis") {
    let sum = 0
    for (let v of values) sum += v as number
    return clampNum(sum, -1, 1)
  }
  let x = 0
  let y = 0
  for (let v of values) {
    x += (v as Vec2)[0]
    y += (v as Vec2)[1]
  }
  let len = Math.hypot(x, y)
  return len > 1 ? [x / len, y / len] : [x, y]
}

function checkValue(what: string, kind: AxisKind, v: unknown): void {
  if (kind === "axis") {
    if (typeof v !== "number" || !Number.isFinite(v)) throw new Error(`${what}: expected a finite number, got ${String(v)}`)
    return
  }
  if (!Array.isArray(v) || v.length !== 2 || !Number.isFinite(v[0]) || !Number.isFinite(v[1])) {
    throw new Error(`${what}: expected [x, y] finite numbers, got ${JSON.stringify(v)}`)
  }
}

/**
 * Declare a control's axes: `createAxes({ rotate: "vec2", zoom: "axis" },
 * { onNudge, onBegin, onEnd })`. The hooks are how nudges reach the
 * control's pose; rates are read back with `rate()` from its update(dt).
 */
export function createAxes<A extends AxesDecl>(kinds: A, hooks: AxesHooks<A> = {}): Axes<A> {
  for (let [name, kind] of Object.entries(kinds)) {
    if (kind !== "axis" && kind !== "vec2") throw new Error(`createAxes: axis "${name}" has kind "${String(kind)}", expected "axis" or "vec2"`)
  }
  let check = (name: string): AxisKind => {
    let kind = kinds[name]
    if (!kind) throw new Error(`createAxes: no axis "${name}" (declared: ${Object.keys(kinds).join(", ")})`)
    return kind
  }
  // Sources per axis: a plain list is the truth (a write and a read in
  // the same synchronous block must agree, which a signal's deferred
  // write does not give), and a version signal notifies the reactive
  // readers - active() and whoever tracks rate() - of a change.
  let sources = new Map<string, (() => number | Vec2)[]>()
  // ownedWrite: add() runs from component bodies (a map's drive()).
  let [version, setVersion] = createSignal(0, { ownedWrite: true })
  let bump = () => setVersion(untrack(version) + 1)
  for (let name of Object.keys(kinds)) sources.set(name, [])
  let rate = (name: string): number | Vec2 => {
    version()
    let values: (number | Vec2)[] = []
    for (let read of sources.get(name)!) values.push(read())
    return combineRates(kinds[name]!, values)
  }
  let active = createMemo(() => {
    for (let [name, kind] of Object.entries(kinds)) {
      let v = rate(name)
      if (kind === "axis" ? v !== 0 : (v as Vec2)[0] !== 0 || (v as Vec2)[1] !== 0) return true
    }
    return false
  })
  let open = new Map<string, number>()
  return {
    kinds,
    add(name, read) {
      check(name)
      if (typeof read !== "function") throw new Error(`createAxes: add("${name}") expects a function`)
      let list = sources.get(name)!
      list.push(read)
      bump()
      return () => {
        let i = list.indexOf(read)
        if (i < 0) return
        list.splice(i, 1)
        bump()
      }
    },
    rate(name) {
      check(name)
      return rate(name) as never
    },
    active,
    nudge(name, delta, focal) {
      let kind = check(name)
      checkValue(`nudge("${name}")`, kind, delta)
      if (focal !== undefined) checkValue(`nudge("${name}") focal`, "vec2", focal)
      hooks.onNudge?.(name, delta, focal)
    },
    begin(name) {
      check(name)
      let depth = open.get(name) ?? 0
      open.set(name, depth + 1)
      if (depth === 0) hooks.onBegin?.(name)
    },
    end(name) {
      check(name)
      let depth = open.get(name) ?? 0
      if (depth === 0) return
      open.set(name, depth - 1)
      if (depth === 1) hooks.onEnd?.(name)
    },
    inGesture(name) {
      check(name)
      return (open.get(name) ?? 0) > 0
    },
  }
}
