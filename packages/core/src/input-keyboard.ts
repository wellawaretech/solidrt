// The keyboard as input sources: a key as a button, a key pair as an
// axis, four keys as a vec2 (the WASD composite), each holding its own
// pressed state from the key events an input map forwards to it
// (InputMap.handlers, spread on the window or the focused leaf). Keys
// match on the physical code ("KeyW", layout-independent) or the logical
// key ("w", what a synthetic event or an odd layout reports), and a
// letter code matches its letter either way, so "KeyW" and "w" both take
// a shifted "W".
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

// A key spec matches its physical code or its logical key; a letter code
// ("KeyW") also matches the logical letter ("w", "W"), so a synthetic
// event without a code (an agent's send_input, an odd layout) still walks.
let matches = (event: KeyEvent, key: string): boolean => {
  if (event.code === key || event.key === key) return true
  if (event.key.length !== 1) return false
  if (key.length === 1) return event.key.toLowerCase() === key.toLowerCase()
  return key.length === 4 && key.startsWith("Key") && event.key.toLowerCase() === key[3]!.toLowerCase()
}

// A set of keys with a reactive "how many are held" count.
function held(keys: string[]) {
  let down = new Set<string>()
  // ownedWrite: a map forwards key events from wherever its handlers sit.
  let [count, setCount] = createSignal(0, { ownedWrite: true })
  return {
    count,
    key(event: KeyEvent, isDown: boolean) {
      for (let k of keys) {
        if (!matches(event, k)) continue
        if (isDown) down.add(k)
        else down.delete(k)
      }
      setCount(down.size)
    },
    blur() {
      down.clear()
      setCount(0)
    },
    has(k: string) {
      return down.has(k)
    },
  }
}

function checkKey(what: string, key: unknown): void {
  if (typeof key !== "string" || key.length === 0) throw new Error(`keyboard.${what}: expected a key code or key name, got ${String(key)}`)
}

/** One key as a button source (pressed while held). */
function key(code: string): InputSource<"button"> {
  checkKey("key", code)
  let state = held([code])
  return {
    kind: "button",
    label: `keyboard ${code}`,
    rate: () => state.count() > 0,
    key: state.key,
    blur: state.blur,
  }
}

/** Two keys as an axis: `neg` reads -1, `pos` reads 1, both 0. */
function axis(neg: string, pos: string): InputSource<"axis"> {
  checkKey("axis", neg)
  checkKey("axis", pos)
  let state = held([neg, pos])
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
  for (let side of ["up", "down", "left", "right"] as const) checkKey(`vec2 ${side}`, keys[side])
  let state = held([keys.up, keys.down, keys.left, keys.right])
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
 * Each call makes an independent source with its own held state; the
 * composites are shared singletons.
 */
export let keyboard = {
  key,
  axis,
  vec2,
  wasd: vec2({ up: "KeyW", down: "KeyS", left: "KeyA", right: "KeyD" }),
  arrows: vec2({ up: "ArrowUp", down: "ArrowDown", left: "ArrowLeft", right: "ArrowRight" }),
}

export type KeyboardDevice = typeof keyboard
