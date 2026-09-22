// Processors and interactions: sources made from sources. A processor
// changes a value (Unity's Invert and Scale), an interaction recognizes a
// pattern on a button over time (Unity's Hold, Tap and MultiTap
// interactions, Unreal's Triggers) and is itself a button source, so an
// action binds `hold(pad.button("west"))` for a charged attack next to
// the plain press on another action, and the consumer reads one bool.
//
// Timing: an interaction watches its source's rate under a root of its
// own and keeps time with setTimeout, so it works headless and needs no
// frame loop; the root is disposed with the creating scope (a component
// body) and lives for the session without one. `tap` and `doubleTap`
// are edges, not levels: they read pressed for one task, long enough
// for a map's onPress to fire, then release on their own.
//
// Every processor composes the source's id (`invert(gamepad:leftStick)`,
// `hold(400,keyboard:key:Space)`), so a saved binding restores through
// the same processor (input-id.ts), and carries its device through for
// InputMap.device(). A processor's key/blur hooks forward to the source,
// so a keyboard source keeps receiving the map's key events under one.

import { createEffect, createRoot, createSignal, getOwner, onCleanup } from "@solidjs/signals"
import type { ActionKind, InputSource } from "./input-map"
import type { Vec2 } from "./input-axes"

// Time a button must stay down before hold() reads pressed (Unity's Hold
// interaction default).
const HOLD_MS = 400
// Longest press that still counts as a tap (Unity's Tap default).
const TAP_MS = 200
// Longest gap between two taps that makes a double tap.
const DOUBLE_TAP_GAP_MS = 300

export function checkSource(source: unknown): InputSource {
  let s = source as InputSource
  // A pointer gesture is a callable source (`pointer.drag("Ctrl")` is its chord variant).
  if (!s || (typeof s !== "object" && typeof s !== "function") || (s.kind !== "button" && s.kind !== "axis" && s.kind !== "vec2") || typeof s.label !== "string" || typeof s.id !== "string") {
    throw new Error(`createInputMap: not an input source: ${String(source)}`)
  }
  return s
}

let checkAxisSource = (what: string, source: InputSource): void => {
  checkSource(source)
  if ((source.kind as ActionKind) === "button") throw new Error(`${what}: "${source.label}" is a button`)
}

let checkButtonSource = (what: string, source: InputSource): void => {
  checkSource(source)
  if (source.kind !== "button") throw new Error(`${what}: "${source.label}" is a ${source.kind}, not a button`)
}

let checkMs = (what: string, ms: number): void => {
  if (!Number.isFinite(ms) || ms < 0) throw new Error(`${what}: ms must be a non-negative number, got ${String(ms)}`)
}

/**
 * A source with its values negated - Unity's Invert processor. The one
 * convention split every engine has: a drag moves the CONTENT under the
 * finger, while keys and sticks move the CAMERA, so a preset binds the
 * arrows or a stick to `pan` or `rotate` through invert() and the axis
 * keeps one meaning.
 */
export function invert<K extends "axis" | "vec2">(source: InputSource<K>): InputSource<K> {
  checkAxisSource("invert", source)
  let neg = (v: number | Vec2): number | Vec2 => (typeof v === "number" ? -v : [-v[0], -v[1]])
  return {
    kind: source.kind,
    label: `${source.label} (inverted)`,
    id: `invert(${source.id})`,
    device: source.device,
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
  checkAxisSource("scale", source)
  if (!Number.isFinite(factor)) throw new Error(`scale: factor must be a finite number, got ${factor}`)
  let mul = (v: number | Vec2): number | Vec2 => (typeof v === "number" ? v * factor : [v[0] * factor, v[1] * factor])
  return {
    kind: source.kind,
    label: `${source.label} (x${factor})`,
    id: `scale(${factor},${source.id})`,
    device: source.device,
    rate: source.rate ? () => mul(source.rate!() as number | Vec2) as never : undefined,
    deltas: source.deltas
      ? sink => source.deltas!({ begin: sink.begin, end: sink.end, delta: (value, focal) => sink.delta(mul(value), focal) })
      : undefined,
    key: source.key,
    blur: source.blur,
  }
}

// A button source derived from one button source's press and release
// edges: `edge(down, now)` runs on each, with `set` for the derived
// state and `later` for a timer that the disposal clears.
type EdgeHooks = { set: (pressed: boolean) => void; later: (ms: number, run: () => void) => void; clearLater: () => void }

function derived(source: InputSource<"button">, label: string, id: string, edge: (down: boolean, now: number, hooks: EdgeHooks) => void): InputSource<"button"> {
  // ownedWrite: the edges land inside an effect and timers.
  let [pressed, setPressed] = createSignal(false, { ownedWrite: true })
  let timer: ReturnType<typeof setTimeout> | null = null
  let hooks: EdgeHooks = {
    set: setPressed,
    later(ms, run) {
      hooks.clearLater()
      timer = setTimeout(() => {
        timer = null
        run()
      }, ms)
    },
    clearLater() {
      if (timer !== null) clearTimeout(timer)
      timer = null
    },
  }
  let dispose = createRoot(dispose => {
    createEffect(
      () => source.rate!(),
      (down, prev) => {
        if (down !== prev) edge(down, performance.now(), hooks)
      },
      { defer: true },
    )
    return dispose
  })
  if (getOwner()) {
    onCleanup(() => {
      hooks.clearLater()
      dispose()
    })
  }
  return { kind: "button", label, id, device: source.device, rate: pressed, key: source.key, blur: source.blur }
}

// A one-task pulse: pressed now, released on the next task, so a map's
// onPress effect (which runs at the flush in between) sees the edge.
let pulse = (hooks: EdgeHooks): void => {
  hooks.set(true)
  hooks.later(0, () => hooks.set(false))
}

/** Pressed once `source` has been held `ms` (default 400), until released:
 * a charged attack, a long-press menu. */
export function hold(source: InputSource<"button">, ms = HOLD_MS): InputSource<"button"> {
  checkButtonSource("hold", source)
  checkMs("hold", ms)
  return derived(source, `${source.label} (hold ${ms} ms)`, `hold(${ms},${source.id})`, (down, _now, hooks) => {
    if (down) hooks.later(ms, () => hooks.set(true))
    else {
      hooks.clearLater()
      hooks.set(false)
    }
  })
}

/** A press released within `ms` (default 200): pressed for one task on the
 * release, so onPress fires once per tap and a long press is not one. */
export function tap(source: InputSource<"button">, ms = TAP_MS): InputSource<"button"> {
  checkButtonSource("tap", source)
  checkMs("tap", ms)
  let downAt = 0
  return derived(source, `${source.label} (tap)`, `tap(${ms},${source.id})`, (down, now, hooks) => {
    if (down) downAt = now
    else if (now - downAt <= ms) pulse(hooks)
  })
}

/** Two taps (each within `tapMs`, default 200) at most `gapMs` (default
 * 300) apart: pressed for one task on the second release. */
export function doubleTap(source: InputSource<"button">, gapMs = DOUBLE_TAP_GAP_MS, tapMs = TAP_MS): InputSource<"button"> {
  checkButtonSource("doubleTap", source)
  checkMs("doubleTap", gapMs)
  checkMs("doubleTap", tapMs)
  let downAt = 0
  let lastTap = -Infinity
  return derived(source, `${source.label} (double tap)`, `doubleTap(${gapMs},${tapMs},${source.id})`, (down, now, hooks) => {
    if (down) {
      downAt = now
      return
    }
    if (now - downAt > tapMs) return
    if (now - lastTap <= gapMs) {
      lastTap = -Infinity
      pulse(hooks)
    } else lastTap = now
  })
}

/** Pressed while every source is: Shift+click on one action, a two-button
 * combo on a pad. Key events reach each keyboard source among them. */
export function chord(...sources: InputSource<"button">[]): InputSource<"button"> {
  if (sources.length < 2) throw new Error("chord: needs at least two button sources")
  for (let s of sources) checkButtonSource("chord", s)
  return {
    kind: "button",
    label: sources.map(s => s.label).join(" + "),
    id: `chord(${sources.map(s => s.id).join(",")})`,
    device: sources[0]!.device,
    rate: () => sources.every(s => s.rate?.() === true),
    key: (event, down) => {
      for (let s of sources) s.key?.(event, down)
    },
    blur: () => {
      for (let s of sources) s.blur?.()
    },
  }
}
