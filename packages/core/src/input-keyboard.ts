// The keyboard as input sources: a key as a button, a key pair as an
// axis, four keys as a vec2 (the WASD composite), each holding its own
// pressed state from the key events an input map forwards to it
// (InputMap.handlers, spread on the window or the focused leaf). Keys
// match on the physical code ("KeyW", layout-independent) or the logical
// key ("w", what a synthetic event or an odd layout reports), and a
// letter code matches its letter either way, so "KeyW" and "w" both take
// a shifted "W".
//
// A spec may name modifiers ahead of the key ("Shift+Tab", "Ctrl+KeyS";
// Shift, Ctrl, Alt, Meta): the down must carry them, while a bare spec
// ignores them (a shifted W still walks). Within one source the most
// specific matching spec wins a down, so `axis("Shift+Tab", "Tab")` reads
// -1, not 0, on Shift+Tab; and an up releases on the key alone, so a
// modifier let go first cannot leave the key stuck.
//
// Held state is a count behind a signal, so a source's rate is reactive
// (a held key wakes a control's frame loop) and a blur - the up that never
// arrives once focus has left - clears it. Composites clamp nothing: the
// map's combination does (W and D together read [1, -1] before the
// vec2 clamp to unit length, so diagonals walk at the same speed).
//
// Values use the screen convention (x right, y down): W reads [0, -1], as
// a stick pushed up does.

import { createSignal } from "@solidjs/signals"
import type { KeyEvent } from "./types"
import type { InputSource } from "./input-map"
import type { Vec2 } from "./input-axes"

type Modifier = "shiftKey" | "ctrlKey" | "altKey" | "metaKey"

const MODIFIERS: Record<string, Modifier> = { Shift: "shiftKey", Ctrl: "ctrlKey", Control: "ctrlKey", Alt: "altKey", Meta: "metaKey" }

// A parsed spec: the text as given (the held-state key and the label),
// the key it names, and the modifiers the down must carry.
type Spec = { text: string; key: string; mods: Modifier[] }

function parse(what: string, text: unknown): Spec {
  if (typeof text !== "string" || text.length === 0) throw new Error(`keyboard.${what}: expected a key code or key name, got ${String(text)}`)
  let parts = text.split("+")
  let key = parts.pop()!
  if (key.length === 0) throw new Error(`keyboard.${what}: "${text}" names no key`)
  let mods: Modifier[] = []
  for (let part of parts) {
    let mod = MODIFIERS[part]
    if (!mod) throw new Error(`keyboard.${what}: unknown modifier "${part}" in "${text}" (Shift, Ctrl, Alt or Meta)`)
    if (!mods.includes(mod)) mods.push(mod)
  }
  return { text, key, mods }
}

// A key matches its physical code or its logical key; a letter code
// ("KeyW") also matches the logical letter ("w", "W") and "Space" the
// logical " ", so a synthetic event without a code (an agent's
// send_input, an odd layout) still walks.
let matches = (event: KeyEvent, key: string): boolean => {
  if (event.code === key || event.key === key) return true
  if (key === "Space" && event.key === " ") return true
  if (event.key.length !== 1) return false
  if (key.length === 1) return event.key.toLowerCase() === key.toLowerCase()
  return key.length === 4 && key.startsWith("Key") && event.key.toLowerCase() === key[3]!.toLowerCase()
}

// A set of specs with a reactive "how many are held" count.
function held(specs: Spec[]) {
  let down = new Set<string>()
  // ownedWrite: a map forwards key events from wherever its handlers sit.
  let [count, setCount] = createSignal(0, { ownedWrite: true })
  return {
    count,
    key(event: KeyEvent, isDown: boolean) {
      // Every spec on this key is released first: a down settles the key
      // on its most specific matches (a repeat under a newly held Shift
      // moves "Tab" to "Shift+Tab" and back), an up frees them all.
      let onKey = specs.filter(s => matches(event, s.key))
      for (let s of onKey) down.delete(s.text)
      if (isDown) {
        let hits = onKey.filter(s => s.mods.every(m => event[m]))
        let most = hits.reduce((n, s) => Math.max(n, s.mods.length), 0)
        for (let s of hits) if (s.mods.length === most) down.add(s.text)
      }
      setCount(down.size)
    },
    blur() {
      down.clear()
      setCount(0)
    },
    has(text: string) {
      return down.has(text)
    },
  }
}

/** One key as a button source (pressed while held). */
function key(spec: string): InputSource<"button"> {
  let state = held([parse("key", spec)])
  return {
    kind: "button",
    label: `keyboard ${spec}`,
    rate: () => state.count() > 0,
    key: state.key,
    blur: state.blur,
  }
}

/** Two keys as an axis: `neg` reads -1, `pos` reads 1, both 0. */
function axis(neg: string, pos: string): InputSource<"axis"> {
  let state = held([parse("axis", neg), parse("axis", pos)])
  return {
    kind: "axis",
    label: `keyboard ${neg}/${pos}`,
    rate: () => {
      state.count()
      return (state.has(pos) ? 1 : 0) - (state.has(neg) ? 1 : 0)
    },
    key: state.key,
    blur: state.blur,
  }
}

export type KeyboardVec2Keys = { up: string; down: string; left: string; right: string }

/** Four keys as a vec2 in the screen convention: up reads [0, -1], right
 * reads [1, 0]; opposite keys cancel. */
function vec2(keys: KeyboardVec2Keys): InputSource<"vec2"> {
  let state = held((["up", "down", "left", "right"] as const).map(side => parse(`vec2 ${side}`, keys[side])))
  return {
    kind: "vec2",
    label: `keyboard ${keys.up}/${keys.left}/${keys.down}/${keys.right}`,
    rate: (): Vec2 => {
      state.count()
      return [(state.has(keys.right) ? 1 : 0) - (state.has(keys.left) ? 1 : 0), (state.has(keys.down) ? 1 : 0) - (state.has(keys.up) ? 1 : 0)]
    },
    key: state.key,
    blur: state.blur,
  }
}

/**
 * The keyboard device: `keyboard.key("Space")`, `keyboard.axis("KeyQ",
 * "KeyE")`, `keyboard.vec2({ up, down, left, right })`, and the two
 * composites every game binds, `keyboard.wasd` and `keyboard.arrows`.
 * A spec may carry modifiers ("Shift+Tab", "Ctrl+KeyS"). Each call makes
 * an independent source with its own held state; the composites are
 * shared singletons.
 */
export let keyboard = {
  key,
  axis,
  vec2,
  wasd: vec2({ up: "KeyW", down: "KeyS", left: "KeyA", right: "KeyD" }),
  arrows: vec2({ up: "ArrowUp", down: "ArrowDown", left: "ArrowLeft", right: "ArrowRight" }),
}

export type KeyboardDevice = typeof keyboard
