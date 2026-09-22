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
//
// A settings screen has three things on the map: save()/load() carry the
// bindings as source ids (input-id.ts) to and from storage, rebind()
// listens for the next input on the devices given and binds it, and
// device() is the device that last moved anything bound, so button
// prompts follow the player from the keyboard to the pad (Unity's control
// schemes, without a second concept: a scheme is the bindings of one
// device, which bindings() lists by source.device).

import { createEffect, createRoot, createSignal, untrack } from "@solidjs/signals"
import type { KeyEvent } from "./types"
import { combineRates } from "./input-axes"
import type { Axes, AxesDecl, AxisValue, Vec2 } from "./input-axes"
import { checkSource } from "./input-processors"
import { resolveSource } from "./input-id"
import { keySpec } from "./input-keyboard"

export type ActionKind = "button" | "axis" | "vec2"
export type ActionValue<K extends ActionKind> = K extends "button" ? boolean : K extends "axis" ? number : Vec2
export type ActionsDecl = Record<string, ActionKind>

/** Where a delta source delivers: the gesture brackets and the deltas.
 * An end may carry the release velocity in the source's units per second
 * (a drag's in element heights per second), the fling fact a consumer
 * glides on; absent or zero when the finger rested. */
export interface DeltaSink {
  begin(): void
  delta(value: number | Vec2, focal?: Vec2): void
  end(velocity?: number | Vec2): void
}

/** The devices a map knows by name: the id prefix of their sources, and
 * what rebind() listens on and load() resolves through. */
export type DeviceName = "keyboard" | "gamepad" | "pointer"

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
  /** The serializable name (input-id.ts: "keyboard:key:Space",
   * "invert(gamepad:leftStick)"): what save() stores and load() resolves.
   * A custom source names itself under a prefix of its own. */
  readonly id: string
  /** The device the source reads, for InputMap.device(); a custom source
   * may leave it out and never counts as the active device. */
  readonly device?: DeviceName
  rate?: () => ActionValue<K>
  deltas?: (sink: DeltaSink) => () => void
  key?: (event: KeyEvent, down: boolean) => void
  blur?: () => void
}

/** What a map asks of a device: sources by spec (the id's device half),
 * and, for rebind(), a listener for the next control of a kind the player
 * actuates. The keyboard has no listener of its own: the map captures
 * the next key through its handlers. */
export interface InputDevice {
  readonly name: DeviceName
  resolve(spec: string): InputSource
  listen?(kind: ActionKind, found: (source: InputSource) => void): () => void
}

export type InputDeviceSet = Partial<Record<DeviceName, InputDevice>>

export type Binding = { action: string; source: InputSource }

export interface GestureListener<K extends "axis" | "vec2" = "axis" | "vec2"> {
  begin?: () => void
  delta?: (value: AxisValue<K>, focal: Vec2 | undefined) => void
  /** `velocity` is the release velocity in the action's units per second
   * when the source measured one (a drag's lift), else undefined. */
  end?: (velocity: AxisValue<K> | undefined) => void
}

/** The half of a keyboard composite a key rebinds: `neg`/`pos` of an
 * axis, `up`/`down`/`left`/`right` of a vec2. */
export type RebindPart = "neg" | "pos" | "up" | "down" | "left" | "right"

export interface RebindOptions {
  /** Cancels the listen; the promise rejects with the signal's reason. */
  signal?: AbortSignal
  /** Unbind the action's sources of the same device first (default true),
   * so a row on a settings screen replaces rather than adds. */
  replace?: boolean
  /** On an axis or vec2 action, which part of the bound keyboard
   * composite a key press replaces; without it keys are ignored there. */
  part?: RebindPart
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
   * the delta channel (`end` with the release velocity flings). */
  set<N extends keyof A & string>(action: N, value: ActionValue<A[N]>): void
  press(action: ButtonActions<A>): void
  release(action: ButtonActions<A>): void
  nudge<N extends AxisActions<A>>(action: N, delta: ActionValue<A[N]>, focal?: Vec2): void
  begin(action: AxisActions<A>): void
  end<N extends AxisActions<A>>(action: N, velocity?: ActionValue<A[N]>): void
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
  /** Reactive: the device that last moved anything bound (a key down, a
   * stick leaving rest, a gesture delta), undefined until one has. The
   * button-prompt switch: show the bindings of this device. */
  device(): DeviceName | undefined
  /** The bindings as source ids per action, in binding order - JSON for
   * storage. Every action is present, an unbound one as []. */
  save(): Record<keyof A & string, string[]>
  /** Replace the bindings of each action in `saved` with the ids resolved
   * through `devices` (actions absent from `saved` keep theirs). Throws
   * on an unknown action, a device not given or an id a device rejects,
   * before changing anything. */
  load(saved: Partial<Record<keyof A & string, string[]>>, devices: InputDeviceSet): void
  /**
   * Listen for the next input on `devices` that fits the action and bind
   * it: a key down (the keyboard, through this map's handlers), a pad
   * button, trigger or stick, a pointer gesture's first movement.
   * Resolves with the source bound. One rebind at a time per map.
   */
  rebind(action: keyof A & string, devices: InputDeviceSet, options?: RebindOptions): Promise<InputSource>
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
  /** The device watchers per bound source (see watchDevice). */
  watch: Map<InputSource, () => void>
}

let compatible = (source: ActionKind, action: ActionKind): boolean => source === action || (source === "button" && action === "axis")

let neutral = (kind: ActionKind): boolean | number | Vec2 => (kind === "button" ? false : kind === "axis" ? 0 : [0, 0])

let isNeutral = (v: boolean | number | Vec2): boolean => (typeof v === "boolean" ? !v : typeof v === "number" ? v === 0 : v[0] === 0 && v[1] === 0)

function checkValue(what: string, kind: ActionKind, v: unknown): void {
  if (kind === "button") {
    if (typeof v !== "boolean") throw new Error(`${what}: expected a boolean, got ${String(v)}`)
  } else if (kind === "axis") {
    if (typeof v !== "number" || !Number.isFinite(v)) throw new Error(`${what}: expected a finite number, got ${String(v)}`)
  } else if (!Array.isArray(v) || v.length !== 2 || !Number.isFinite(v[0]) || !Number.isFinite(v[1])) {
    throw new Error(`${what}: expected [x, y] finite numbers, got ${JSON.stringify(v)}`)
  }
}

// The parts of a keyboard composite spec, by kind, in id order.
const AXIS_PARTS: RebindPart[] = ["neg", "pos"]
const VEC2_PARTS: RebindPart[] = ["up", "down", "left", "right"]

export function createInputMap<A extends ActionsDecl>(actions: A): InputMap<A> {
  if (!actions || typeof actions !== "object") throw new Error("createInputMap: expected an object of action kinds")
  let states = new Map<string, ActionState>()
  // ownedWrite: the device changes inside the watchers' effects and from
  // gesture deltas delivered by event handlers.
  let [device, setDevice] = createSignal<DeviceName | undefined>(undefined, { ownedWrite: true })
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
      watch: new Map(),
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
  let touched = (source: InputSource | undefined): void => {
    if (source?.device && untrack(device) !== source.device) setDevice(source.device)
  }
  // The delta channel fan-out for one action. Brackets are counted as
  // delivered, so a disable can close what is open and an end whose begin
  // was dropped (or never delivered) reaches no listener. A delta from a
  // device source also names it the active device.
  let closeGesture = (s: ActionState, velocity?: number | Vec2): void => {
    s.depth--
    s.gesture.forEach(g => g.end?.(velocity as never))
  }
  let sink = (s: ActionState, source?: InputSource): DeltaSink => ({
    begin: () => {
      if (!s.enabled) return
      s.depth++
      s.gesture.forEach(g => g.begin?.())
    },
    delta: (value, focal) => {
      touched(source)
      if (!s.enabled) return
      s.gesture.forEach(g => g.delta?.(value as never, focal))
    },
    end: velocity => {
      if (s.depth > 0) closeGesture(s, velocity)
    },
  })
  // A rate source leaving rest names its device the active one: one
  // effect per binding, under a root disposed at unbind.
  let watchDevice = (source: InputSource): (() => void) =>
    createRoot(dispose => {
      createEffect(
        () => source.rate!(),
        v => {
          if (!isNeutral(v)) touched(source)
        },
        { defer: true },
      )
      return dispose
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

  let checkBind = (name: string, source: InputSource): ActionState => {
    let s = state(name)
    checkSource(source)
    if (!compatible(source.kind, s.kind)) throw new Error(`createInputMap: cannot bind ${source.kind} source "${source.label}" to ${s.kind} action "${name}"`)
    return s
  }
  let bindOne = (name: string, source: InputSource): void => {
    let s = checkBind(name, source)
    if (s.sources.includes(source)) return
    s.sources.push(source)
    s.bump()
    if (source.deltas && s.kind !== "button") s.live.set(source, source.deltas(sink(s, source)))
    if (source.rate && source.device) s.watch.set(source, watchDevice(source))
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
    let unwatch = s.watch.get(source)
    if (unwatch) {
      s.watch.delete(source)
      unwatch()
    }
  }
  let edge = (name: string, want: boolean, callback: () => void): (() => void) => {
    let s = buttonState(name, want ? "onPress" : "onRelease")
    if (typeof callback !== "function") throw new Error(`createInputMap: ${want ? "onPress" : "onRelease"}("${name}") expects a function`)
    // The edge is relative to the value at registration, kept here rather
    // than read from the effect's previous value: a deferred effect created
    // while a pending write sits on the action (a bind in the same tick)
    // runs on that flush with no previous value, which would count a source
    // already held at registration as a press.
    let last = untrack(() => s.value() as boolean)
    return createRoot(dispose => {
      // The callback is a side effect that may read state of its own (a
      // focus, a pose): a snapshot, not a dependency, so untracked.
      createEffect(
        () => s.value() as boolean,
        pressed => {
          if (pressed === want && last !== want) untrack(callback)
          last = pressed
        },
        { defer: true },
      )
      return dispose
    })
  }

  // The pending rebind's key capture: the next key down that is not a
  // bare modifier is its source, and never reaches the bound sources.
  let capture: ((event: KeyEvent) => void) | null = null

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
    end(action, velocity) {
      let s = axisState(action, "end")
      if (velocity !== undefined) checkValue(`end("${action}") velocity`, s.kind, velocity)
      sink(s).end(velocity as number | Vec2 | undefined)
    },
    enable: (...actions) => switchActions(actions, true, "enable"),
    disable: (...actions) => switchActions(actions, false, "disable"),
    enabled(action) {
      let s = state(action)
      s.version()
      return s.enabled
    },
    device,
    save() {
      let out = {} as Record<keyof A & string, string[]>
      for (let [name, s] of states) out[name as keyof A & string] = s.sources.map(source => source.id)
      return out
    },
    load(saved, devices) {
      if (!saved || typeof saved !== "object") throw new Error("createInputMap: load() expects an object of id lists per action")
      if (!devices || typeof devices !== "object") throw new Error("createInputMap: load() expects the devices to resolve through")
      // Resolve everything first: a bad id leaves the map as it was.
      let plan: [string, InputSource[]][] = []
      for (let [name, ids] of Object.entries(saved)) {
        if (!Array.isArray(ids)) throw new Error(`createInputMap: load() action "${name}": expected an array of ids`)
        let sources = ids.map(id => resolveSource(id, devices))
        for (let source of sources) checkBind(name, source)
        plan.push([name, sources])
      }
      for (let [name, sources] of plan) {
        for (let source of [...state(name).sources]) unbindOne(name, source)
        for (let source of sources) bindOne(name, source)
      }
    },
    rebind(action, devices, options = {}) {
      let s = state(action)
      if (!devices || typeof devices !== "object") throw new Error("createInputMap: rebind() expects the devices to listen on")
      if (capture) throw new Error("createInputMap: a rebind is already pending on this map")
      let { signal, replace = true, part } = options
      let parts = s.kind === "axis" ? AXIS_PARTS : s.kind === "vec2" ? VEC2_PARTS : []
      if (part !== undefined) {
        if (s.kind === "button") throw new Error(`createInputMap: rebind("${action}") part: "${action}" is a button, a key rebinds it whole`)
        if (!parts.includes(part)) throw new Error(`createInputMap: rebind("${action}") part "${part}": a ${s.kind} action has ${parts.join("/")}`)
      }
      if (!devices.keyboard && !devices.gamepad?.listen && !devices.pointer?.listen) throw new Error(`createInputMap: rebind("${action}") has no device to listen on`)
      if (signal?.aborted) return Promise.reject(signal.reason ?? new Error("rebind aborted"))
      return new Promise<InputSource>((resolve, reject) => {
        let stops: (() => void)[] = []
        let settled = false
        // The signal's onabort is a single handler property (the runtime's
        // AbortSignal subset): chain onto whatever the app set and restore it.
        let prevAbort = signal?.onabort ?? null
        let cleanup = () => {
          settled = true
          capture = null
          if (signal) signal.onabort = prevAbort
          for (let stop of stops) stop()
        }
        let abort = () => {
          if (settled) return
          cleanup()
          reject(signal!.reason ?? new Error("rebind aborted"))
        }
        // A found source arrives from inside a device's effect or an event
        // handler: apply on the microtask, outside the notifier.
        let found = (source: InputSource, replacing: boolean) => {
          if (settled) return
          settled = true
          queueMicrotask(() => {
            try {
              cleanup()
              if (replacing) for (let bound of [...s.sources]) if (bound.device === source.device) unbindOne(action, bound)
              bindOne(action, source)
              resolve(source)
            } catch (err) {
              reject(err)
            }
          })
        }
        // The keyboard: a key rebinds a button action whole; on an axis or
        // vec2 action it replaces one part of the bound composite, whose
        // other parts stay.
        let keyboard = devices.keyboard
        if (keyboard) {
          capture = event => {
            let spec = keySpec(event)
            if (spec === null) return
            if (s.kind === "button") {
              found(keyboard.resolve(`key:${spec}`), replace)
              return
            }
            if (part === undefined) return
            let prefix = `${s.kind}:`
            let composite = s.sources.find(b => b.device === "keyboard" && b.id.startsWith(`keyboard:${prefix}`))
            if (!composite) {
              settled = true
              cleanup()
              reject(new Error(`createInputMap: rebind("${action}") part "${part}": no keyboard ${s.kind} composite is bound to "${action}"`))
              return
            }
            let specs = composite.id.slice(`keyboard:${prefix}`.length).split("/")
            specs[parts.indexOf(part)] = spec
            let next = keyboard.resolve(`${prefix}${specs.join("/")}`)
            // Only that composite is replaced, whatever `replace` says: a
            // second keyboard composite on the action (arrows next to
            // WASD) is a binding of its own, and two copies would add up.
            unbindOne(action, composite)
            found(next, false)
          }
        }
        for (let name of ["gamepad", "pointer"] as const) {
          let d = devices[name]
          if (d?.listen) {
            stops.push(
              d.listen(s.kind, source => {
                if (compatible(source.kind, s.kind)) found(source, replace)
              }),
            )
          }
        }
        if (signal) {
          signal.onabort = event => {
            prevAbort?.(event)
            abort()
          }
        }
      })
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
            end: velocity => axes.end(axis, velocity as never),
          }),
        )
      }
      return () => {
        for (let stop of stops) stop()
      }
    },
  }
  // A key event reaches each bound keyboard source once, however many
  // actions it is bound to. While a rebind listens, a key down is
  // captured instead: the player is naming a key, not playing.
  let forwardKey = (event: KeyEvent, down: boolean): void => {
    if (capture && down && !event.repeat) {
      capture(event)
      return
    }
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
