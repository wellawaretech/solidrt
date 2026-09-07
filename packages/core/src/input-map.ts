// The input map: named, typed actions between devices and consumers -
// Godot's InputMap and Unity's action maps, in reactive form. An app
// declares its actions once (`look: "vec2"`, `jump: "button"`), binds
// device sources to them (a stick, a key composite, a pointer drag), and
// consumers read the action, never the device. Anything can then drive a
// consumer: a second pad on a second map for split screen, a debug
// command or a network peer through the by-name injection, a replay on
// the action stream, a settings screen over bindings().
//
// Each axis or vec2 action carries two channels, which is the part Godot
// (no mouse motion in actions) and Unity (mouse delta and stick share one
// value, consumers scale by dt themselves) each got half of:
// - the RATE: the sum of every bound source's current value, clamped (an
//   axis to -1..1, a vec2 to unit length), reactive, sampled per frame by
//   the consumer and integrated over dt at its own speed;
// - the DELTAS: immediate, device-free amounts a gesture produced (a drag
//   in view heights, a wheel notch in octaves), delivered as they happen
//   with begin/end brackets around a finger's gesture.
// Values use the screen convention: x right, y down.
//
// A button action reads pressed while any bound button source is, and
// may also bind to an axis action (1 while pressed). Kinds are checked at
// bind: a vec2 source on an axis action throws.
//
// Reactivity: rates are memos over reactive sources, so a consumer's
// active() gate wakes on a stick or a key. Create the map in an owned
// scope (a component body): onPress/onRelease run effects under it.
//
// Consumers with the Axes contract (createAxes; the camera controls) are
// connected by name with drive(): each axis takes the action of the same
// name, or the name given, for both channels.
//
// Contexts (a menu open, a cutscene) switch actions off and on by name:
// enable/disable. A disabled action reads neutral, drops its deltas and
// closes the gesture it had open, while its sources keep their state, so
// a key still held when the action comes back reads at once. A set is a
// list of names, which a preset's action object already is
// (`input.disable(...Object.keys(gameActions))`): Unity's per-map enable
// and Unreal's stacked contexts, on one map, with no extra concept.

import { createEffect, createRoot, createSignal, untrack } from "@solidjs/signals"
import type { KeyEvent } from "./types"
import { combineRates } from "./input-axes"
import type { Axes, AxesDecl, AxisValue, Vec2 } from "./input-axes"

export type ActionKind = "button" | "axis" | "vec2"
export type ActionValue<K extends ActionKind> = K extends "button" ? boolean : K extends "axis" ? number : Vec2
export type ActionsDecl = Record<string, ActionKind>

/** Where a delta source delivers: the gesture brackets and the deltas. */
export interface DeltaSink {
  begin(): void
  delta(value: number | Vec2, focal?: Vec2): void
  end(): void
}

/**
 * One device control as an input source. `rate` reads its current value
 * (reactive); `deltas` subscribes a sink to the amounts a gesture
 * produces; `key`/`blur` receive the key events a map forwards
 * (InputMap.handlers). A source may carry any subset.
 */
export interface InputSource<K extends ActionKind = ActionKind> {
  readonly kind: K
  /** For bindings listings and settings screens: "keyboard W/A/S/D". */
  readonly label: string
  rate?: () => ActionValue<K>
  deltas?: (sink: DeltaSink) => () => void
  key?: (event: KeyEvent, down: boolean) => void
  blur?: () => void
}

export type Binding = { action: string; source: InputSource }

/**
 * A source with its values negated - Unity's Invert processor. The one
 * convention split every engine has: a drag moves the CONTENT under the
 * finger, while keys and sticks move the CAMERA, so a preset binds the
 * arrows or a stick to `pan` or `rotate` through invert() and the axis
 * keeps one meaning.
 */
export function invert<K extends "axis" | "vec2">(source: InputSource<K>): InputSource<K> {
  checkSource(source)
  if ((source.kind as ActionKind) === "button") throw new Error(`invert: "${source.label}" is a button`)
  let neg = (v: number | Vec2): number | Vec2 => (typeof v === "number" ? -v : [-v[0], -v[1]])
  return {
    kind: source.kind,
    label: `${source.label} (inverted)`,
    rate: source.rate ? () => neg(source.rate!() as number | Vec2) as never : undefined,
    deltas: source.deltas
      ? sink => source.deltas!({ begin: sink.begin, end: sink.end, delta: (value, focal) => sink.delta(neg(value), focal) })
      : undefined,
    key: source.key,
    blur: source.blur,
  }
}

/** A source scaled by `factor` (rates and deltas alike) - Unity's Scale
 * processor: a slower stick, a finer wheel. */
export function scale<K extends "axis" | "vec2">(source: InputSource<K>, factor: number): InputSource<K> {
  checkSource(source)
  if ((source.kind as ActionKind) === "button") throw new Error(`scale: "${source.label}" is a button`)
  if (!Number.isFinite(factor)) throw new Error(`scale: factor must be a finite number, got ${factor}`)
  let mul = (v: number | Vec2): number | Vec2 => (typeof v === "number" ? v * factor : [v[0] * factor, v[1] * factor])
  return {
    kind: source.kind,
    label: `${source.label} (x${factor})`,
    rate: source.rate ? () => mul(source.rate!() as number | Vec2) as never : undefined,
    deltas: source.deltas
      ? sink => source.deltas!({ begin: sink.begin, end: sink.end, delta: (value, focal) => sink.delta(mul(value), focal) })
      : undefined,
    key: source.key,
    blur: source.blur,
  }
}

export interface GestureListener<K extends "axis" | "vec2" = "axis" | "vec2"> {
  begin?: () => void
  delta?: (value: AxisValue<K>, focal: Vec2 | undefined) => void
  end?: () => void
}

type AxisActions<A extends ActionsDecl> = { [N in keyof A]: A[N] extends "button" ? never : N }[keyof A] & string
type ButtonActions<A extends ActionsDecl> = { [N in keyof A]: A[N] extends "button" ? N : never }[keyof A] & string

export interface InputMap<A extends ActionsDecl> {
  readonly actions: A
  /** Bind sources to an action (kinds checked), or a list of bindings (a
   * preset's return). Returns the remover for what this call bound. */
  bind<N extends keyof A & string>(action: N, ...sources: InputSource[]): () => void
  bind(bindings: Binding[]): () => void
  unbind(action: keyof A & string, source: InputSource): void
  /** Current bindings, of one action or all, in binding order. */
  bindings(action?: keyof A & string): Binding[]
  /** The action's rate now (reactive): pressed for a button, -1..1 for an
   * axis, a unit-clamped [x, y] for a vec2. */
  value<N extends keyof A & string>(action: N): ActionValue<A[N]>
  /** Reactive: whether a button action is pressed. */
  pressed(action: ButtonActions<A>): boolean
  /** Edge callbacks on a button action; return the unsubscribe. */
  onPress(action: ButtonActions<A>, callback: () => void): () => void
  onRelease(action: ButtonActions<A>, callback: () => void): () => void
  /** Subscribe to an axis or vec2 action's delta channel. */
  onGesture<N extends AxisActions<A>>(action: N, listener: GestureListener<A[N] extends "axis" ? "axis" : "vec2">): () => void
  /** Injection by name, for scripts, debug commands, peers: `set` holds
   * a rate until set again (a value source of the map's own); `press`/
   * `release` do the same for a button; `nudge`, `begin` and `end` feed
   * the delta channel. */
  set<N extends keyof A & string>(action: N, value: ActionValue<A[N]>): void
  press(action: ButtonActions<A>): void
  release(action: ButtonActions<A>): void
  nudge<N extends AxisActions<A>>(action: N, delta: ActionValue<A[N]>, focal?: Vec2): void
  begin(action: AxisActions<A>): void
  end(action: AxisActions<A>): void
  /** Switch actions on and off by name - a context. A disabled action
   * reads neutral (false, 0, [0, 0]), drops its deltas and closes the
   * gesture it had open (its listeners see the end); its sources keep
   * their state, so a key still held when the action comes back reads at
   * once. Edge callbacks see the switch as a release or a press. All
   * actions start enabled. */
  enable(...actions: (keyof A & string)[]): void
  disable(...actions: (keyof A & string)[]): void
  /** Reactive: whether the action is enabled. */
  enabled(action: keyof A & string): boolean
  /** Key events for the bound keyboard sources: spread on the window
   * (app-global) or on the leaf that should hold focus for them. */
  handlers: {
    onKeyDown(event: KeyEvent): void
    onKeyUp(event: KeyEvent): void
    onBlur(): void
  }
  /**
   * Drive a control's axes (createAxes) from this map: each axis reads
   * the action of the same name, or the one `names` gives it (null skips
   * an axis). Both channels: rates as a source, deltas and brackets as
   * nudges. Returns the disconnect.
   */
  drive<X extends AxesDecl>(axes: Axes<X>, names?: Partial<Record<keyof X & string, keyof A & string | null>>): () => void
}

// Per action: the bound sources and the script value are plain state (a
// write and a read in the same synchronous block must agree, which a
// signal's deferred write does not give); `version` notifies the reactive
// readers of a change.
type ActionState = {
  kind: ActionKind
  sources: InputSource[]
  script: boolean | number | Vec2 | null
  enabled: boolean
  /** Gesture brackets delivered to the listeners and not yet closed. */
  depth: number
  version: () => number
  bump: () => void
  value: () => boolean | number | Vec2
  gesture: Set<GestureListener>
  /** Live delta subscriptions per bound source. */
  live: Map<InputSource, () => void>
}

let compatible = (source: ActionKind, action: ActionKind): boolean => source === action || (source === "button" && action === "axis")

let neutral = (kind: ActionKind): boolean | number | Vec2 => (kind === "button" ? false : kind === "axis" ? 0 : [0, 0])

function checkValue(what: string, kind: ActionKind, v: unknown): void {
  if (kind === "button") {
    if (typeof v !== "boolean") throw new Error(`${what}: expected a boolean, got ${String(v)}`)
  } else if (kind === "axis") {
    if (typeof v !== "number" || !Number.isFinite(v)) throw new Error(`${what}: expected a finite number, got ${String(v)}`)
  } else if (!Array.isArray(v) || v.length !== 2 || !Number.isFinite(v[0]) || !Number.isFinite(v[1])) {
    throw new Error(`${what}: expected [x, y] finite numbers, got ${JSON.stringify(v)}`)
  }
}

function checkSource(source: unknown): InputSource {
  let s = source as InputSource
  if (!s || typeof s !== "object" || (s.kind !== "button" && s.kind !== "axis" && s.kind !== "vec2") || typeof s.label !== "string") {
    throw new Error(`createInputMap: not an input source: ${String(source)}`)
  }
  return s
}

export function createInputMap<A extends ActionsDecl>(actions: A): InputMap<A> {
  if (!actions || typeof actions !== "object") throw new Error("createInputMap: expected an object of action kinds")
  let states = new Map<string, ActionState>()
  for (let [name, kind] of Object.entries(actions)) {
    if (kind !== "button" && kind !== "axis" && kind !== "vec2") throw new Error(`createInputMap: action "${name}" has kind "${String(kind)}", expected "button", "axis" or "vec2"`)
    // ownedWrite: bind() and set() run from component bodies.
    let [version, setVersion] = createSignal(0, { ownedWrite: true })
    let state: ActionState = {
      kind,
      sources: [],
      script: null,
      enabled: true,
      depth: 0,
      version,
      bump: () => setVersion(untrack(version) + 1),
      value: (): boolean | number | Vec2 => {
        version()
        if (!state.enabled) return neutral(kind)
        let s = state.script
        if (kind === "button") {
          if (s === true) return true
          return state.sources.some(src => src.rate?.() === true)
        }
        let values: (number | Vec2)[] = []
        if (s !== null) values.push(s as number | Vec2)
        for (let src of state.sources) {
          if (!src.rate) continue
          let v = src.rate()
          if (src.kind === "button") values.push(v ? 1 : 0)
          else values.push(v as number | Vec2)
        }
        return combineRates(kind, values)
      },
      gesture: new Set(),
      live: new Map(),
    }
    states.set(name, state)
  }
  let state = (name: string): ActionState => {
    let s = states.get(name)
    if (!s) throw new Error(`createInputMap: no action "${name}" (declared: ${[...states.keys()].join(", ")})`)
    return s
  }
  let axisState = (name: string, what: string): ActionState => {
    let s = state(name)
    if (s.kind === "button") throw new Error(`createInputMap: ${what} needs an axis or vec2 action, "${name}" is a button`)
    return s
  }
  let buttonState = (name: string, what: string): ActionState => {
    let s = state(name)
    if (s.kind !== "button") throw new Error(`createInputMap: ${what} needs a button action, "${name}" is a ${s.kind}`)
    return s
  }
  // The delta channel fan-out for one action. Brackets are counted as
  // delivered, so a disable can close what is open and an end whose begin
  // was dropped (or never delivered) reaches no listener.
  let closeGesture = (s: ActionState): void => {
    s.depth--
    s.gesture.forEach(g => g.end?.())
  }
  let sink = (s: ActionState): DeltaSink => ({
    begin: () => {
      if (!s.enabled) return
      s.depth++
      s.gesture.forEach(g => g.begin?.())
    },
    delta: (value, focal) => {
      if (!s.enabled) return
      s.gesture.forEach(g => g.delta?.(value as never, focal))
    },
    end: () => {
      if (s.depth > 0) closeGesture(s)
    },
  })
  let switchActions = (names: string[], on: boolean, what: string): void => {
    if (names.length === 0) throw new Error(`createInputMap: ${what}() needs at least one action`)
    for (let name of names) {
      let s = state(name)
      if (s.enabled === on) continue
      s.enabled = on
      while (!on && s.depth > 0) closeGesture(s)
      s.bump()
    }
  }

  let bindOne = (name: string, source: InputSource): void => {
    let s = state(name)
    checkSource(source)
    if (!compatible(source.kind, s.kind)) throw new Error(`createInputMap: cannot bind ${source.kind} source "${source.label}" to ${s.kind} action "${name}"`)
    if (s.sources.includes(source)) return
    s.sources.push(source)
    s.bump()
    if (source.deltas && s.kind !== "button") s.live.set(source, source.deltas(sink(s)))
  }
  let unbindOne = (name: string, source: InputSource): void => {
    let s = state(name)
    let i = s.sources.indexOf(source)
    if (i < 0) return
    s.sources.splice(i, 1)
    s.bump()
    let stop = s.live.get(source)
    if (stop) {
      s.live.delete(source)
      stop()
    }
  }
  let edge = (name: string, want: boolean, callback: () => void): (() => void) => {
    let s = buttonState(name, want ? "onPress" : "onRelease")
    if (typeof callback !== "function") throw new Error(`createInputMap: ${want ? "onPress" : "onRelease"}("${name}") expects a function`)
    return createRoot(dispose => {
      // The callback is a side effect that may read state of its own (a
      // focus, a pose): a snapshot, not a dependency, so untracked.
      createEffect(
        () => s.value() as boolean,
        (pressed, prev) => {
          if (pressed === want && prev !== want) untrack(callback)
        },
        { defer: true },
      )
      return dispose
    })
  }

  let map: InputMap<A> = {
    actions,
    bind(actionOrList: string | Binding[], ...sources: InputSource[]) {
      let list: Binding[] = Array.isArray(actionOrList) ? actionOrList : sources.map(source => ({ action: actionOrList, source }))
      if (list.length === 0) throw new Error("createInputMap: bind() needs at least one source")
      for (let b of list) bindOne(b.action, b.source)
      return () => {
        for (let b of list) unbindOne(b.action, b.source)
      }
    },
    unbind: unbindOne,
    bindings(action) {
      let out: Binding[] = []
      for (let [name, s] of states) {
        if (action !== undefined && name !== action) continue
        for (let source of s.sources) out.push({ action: name, source })
      }
      if (action !== undefined) state(action)
      return out
    },
    value(action) {
      return state(action).value() as never
    },
    pressed(action) {
      return buttonState(action, "pressed").value() as boolean
    },
    onPress: (action, callback) => edge(action, true, callback),
    onRelease: (action, callback) => edge(action, false, callback),
    onGesture(action, listener) {
      let s = axisState(action, "onGesture")
      if (!listener || typeof listener !== "object") throw new Error(`createInputMap: onGesture("${action}") expects a listener object`)
      s.gesture.add(listener as GestureListener)
      return () => {
        s.gesture.delete(listener as GestureListener)
      }
    },
    set(action, value) {
      let s = state(action)
      checkValue(`set("${action}")`, s.kind, value)
      s.script = value
      s.bump()
    },
    press(action) {
      let s = buttonState(action, "press")
      s.script = true
      s.bump()
    },
    release(action) {
      let s = buttonState(action, "release")
      s.script = false
      s.bump()
    },
    nudge(action, delta, focal) {
      let s = axisState(action, "nudge")
      checkValue(`nudge("${action}")`, s.kind, delta)
      if (focal !== undefined) checkValue(`nudge("${action}") focal`, "vec2", focal)
      sink(s).delta(delta as number | Vec2, focal)
    },
    begin(action) {
      sink(axisState(action, "begin")).begin()
    },
    end(action) {
      sink(axisState(action, "end")).end()
    },
    enable: (...actions) => switchActions(actions, true, "enable"),
    disable: (...actions) => switchActions(actions, false, "disable"),
    enabled(action) {
      let s = state(action)
      s.version()
      return s.enabled
    },
    handlers: {
      onKeyDown: event => forwardKey(event, true),
      onKeyUp: event => forwardKey(event, false),
      onBlur: () => {
        let seen = new Set<InputSource>()
        for (let s of states.values()) {
          for (let source of s.sources) {
            if (seen.has(source)) continue
            seen.add(source)
            source.blur?.()
          }
        }
      },
    },
    drive(axes, names) {
      let stops: (() => void)[] = []
      for (let [axis, kind] of Object.entries(axes.kinds)) {
        let mapped = names && axis in names ? names[axis] : axis
        if (mapped === null) continue
        if (mapped === undefined || !states.has(mapped)) {
          throw new Error(`createInputMap: drive() has no action for axis "${axis}" (declared: ${[...states.keys()].join(", ")}); declare it, map it with names, or skip it with null`)
        }
        let s = state(mapped)
        if (s.kind !== kind) throw new Error(`createInputMap: drive() maps ${kind} axis "${axis}" to ${s.kind} action "${mapped}"`)
        stops.push(axes.add(axis, () => s.value() as never))
        stops.push(
          map.onGesture(mapped as never, {
            begin: () => axes.begin(axis),
            delta: (value, focal) => axes.nudge(axis, value as never, focal),
            end: () => axes.end(axis),
          }),
        )
      }
      return () => {
        for (let stop of stops) stop()
      }
    },
  }
  // A key event reaches each bound keyboard source once, however many
  // actions it is bound to.
  let forwardKey = (event: KeyEvent, down: boolean): void => {
    let seen = new Set<InputSource>()
    for (let s of states.values()) {
      for (let source of s.sources) {
        if (seen.has(source) || !source.key) continue
        seen.add(source)
        source.key(event, down)
      }
    }
  }
  return map
}
