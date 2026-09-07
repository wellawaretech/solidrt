import {
  createEffect,
  createInputMap,
  createSignal,
  gamepad,
  getBoundingBoxViewport,
  getFocusables,
  focusedNode,
  getNodePath,
  keyboard,
  onCleanup,
  onLayout,
  setFocus,
  untrack,
} from "@solidrt/core"
import type { Binding, GamepadDevice, InputMap, KeyboardDevice, Vec2 } from "@solidrt/core"

// Focus navigation: the pointer-free control path, so a TV remote, a
// keyboard, or a gamepad can drive an app. Two movement types over the same
// candidates, both steering real focus (setFocus) across the elements
// declaring `focusable`:
//
// - Spatial (`navigate`): judged by on-screen boxes at press time - the
//   nearest candidate with progress in the pressed direction wins, so
//   registration order never matters.
// - Sequential (`cycle`): visual reading order - rows top to bottom, left
//   to right within a row, wrapping at the ends. Derived from the same
//   boxes rather than registration order, which reordering mounts (a <For>
//   shuffle) silently scrambles.
//
// The nav reads no device (ARCHITECTURE.md: controls through an
// abstraction, never direct event handling). It consumes the three UI
// actions of an input map - `navigate` (vec2), `cycle` (axis), `select`
// (button) - the way Flutter's focus widgets consume intents and Godot's
// controls the ui_* actions. A component library must work with nothing
// wired, as those two do, so a nav created bare binds the standard set
// itself (uiBindings over the keyboard and every pad) on a map of its own,
// reachable as `nav.input` for rebinding; given `input`, it consumes the
// app's map, which may hold the app's other actions too. Either way the
// app spreads the map's handlers on the window: keys arrive there only when
// no focused component consumed them, so a focused TextInput keeps its
// caret keys and the Slider its arrows.
//
// A held direction repeats on the nav's own timing over the action's rate
// (a first step on the edge, the next after NAV_REPEAT_DELAY, then every
// NAV_REPEAT_INTERVAL), so pads and remotes walk the way keyboards used to
// through key repeat. Activation is one path: the `select` press runs the
// focused node's action from the registry below, for Enter, Space, the
// remote's center key and the pad's south button alike. Purely additive to
// pointer input: nothing is focused until the first navigation press, and
// pointer presses work unchanged throughout.

type Direction = "up" | "down" | "left" | "right"

// A held direction walks: the first step on the edge, the next after this
// many ms, then one every NAV_REPEAT_INTERVAL ms while it holds.
const NAV_REPEAT_DELAY = 400
const NAV_REPEAT_INTERVAL = 100
// Deflection along the dominant axis of `navigate` (a stick) or on `cycle`
// below which the action reads as no direction; dpad and arrows read 1.
const NAV_THRESHOLD = 0.5

/** The UI actions the focus navigation consumes: `navigate` (vec2, the
 * direction to move focus in the screen convention, x right and y down),
 * `cycle` (axis: +1 steps to the next candidate in reading order, -1 to
 * the previous) and `select` (button: activate the focused control). An
 * app's own map declares them by spreading this object. */
export const uiActions = { navigate: "vec2", cycle: "axis", select: "button" } as const
export type UiActions = typeof uiActions

export type UiDevices = { keyboard?: KeyboardDevice; gamepad?: GamepadDevice }

/**
 * The standard UI bindings: the arrows on `navigate`, Tab and Shift+Tab
 * on `cycle`, Enter, Space and the remote's center key (code "Select") on
 * `select` for the keyboard; the dpad and the left stick on `navigate` and
 * the south button on `select` for a pad. What a bare createFocusNav
 * binds; an app with its own map applies it with `input.bind(...)`.
 */
export function uiBindings(devices: UiDevices): Binding[] {
  let out: Binding[] = []
  let kb = devices.keyboard
  if (kb) {
    out.push(
      { action: "navigate", source: kb.arrows },
      { action: "cycle", source: kb.axis("Shift+Tab", "Tab") },
      { action: "select", source: kb.key("Enter") },
      { action: "select", source: kb.key("Space") },
      { action: "select", source: kb.key("Select") },
    )
  }
  let pad = devices.gamepad
  if (pad) {
    out.push({ action: "navigate", source: pad.dpad }, { action: "navigate", source: pad.leftStick }, { action: "select", source: pad.button("south") })
  }
  return out
}

// Steps on a held value: once on the edge, again after NAV_REPEAT_DELAY,
// then every NAV_REPEAT_INTERVAL until it reads null or changes (a change
// restarts the delay). Runs under the calling scope: the effect and the
// timer die with it.
function repeating<T>(read: () => T | null, step: (value: T) => void): void {
  let current: T | null = null
  let timer: ReturnType<typeof setTimeout> | null = null
  let stop = () => {
    if (timer == null) return
    clearTimeout(timer)
    timer = null
  }
  let later = (value: T, delay: number) => {
    timer = setTimeout(() => {
      step(value)
      later(value, NAV_REPEAT_INTERVAL)
    }, delay)
  }
  // The step reads focus and the candidates for itself, a snapshot, not a
  // dependency: untracked so the strict-mode read diagnostic stays quiet.
  createEffect(read, (value) => {
    if (value === current) return
    current = value
    stop()
    if (value === null) return
    untrack(() => step(value))
    later(value, NAV_REPEAT_DELAY)
  })
  onCleanup(stop)
}

// What "select" means per focusable node, registered by createPress's ref.
// Only the controller path consults it - key activation reaches the focused
// node by bubbling and never comes through here. Package-internal.
let navActions = new Map<number, () => void>()

export function registerNavAction(nodeId: number, action: () => void): () => void {
  navActions.set(nodeId, action)
  return () => {
    if (navActions.get(nodeId) === action) navActions.delete(nodeId)
  }
}

// Open modals register their container here, most recent on top; the top of
// the stack is every nav's default scope, so modals trap navigation with no
// per-app wiring (see Modal). An explicit `scope` option overrides the stack.
// ownedWrite: the pop runs from onCleanup, inside disposal.
let [scopeStack, setScopeStack] = createSignal<{ id: number }[]>([], { ownedWrite: true })

export function pushNavScope(node: { id: number }): () => void {
  setScopeStack((s) => [...s, node])
  return () => setScopeStack((s) => s.filter((n) => n !== node))
}

export interface FocusNavOptions {
  /**
   * The input map to consume the UI actions from: it must declare
   * `navigate`, `cycle` and `select` with the kinds of `uiActions`, and the
   * app binds it (uiBindings or its own) and spreads its handlers on the
   * window. Without one the nav makes its own map bound to the standard set
   * over the keyboard and every pad, which `nav.input` exposes for
   * rebinding.
   */
  input?: InputMap<any>
  /**
   * Restricts reachable candidates to this node's subtree while it returns
   * one, overriding the default (the topmost open Modal, which traps
   * navigation automatically); null/undefined falls back to that default.
   * When a scope appears while focus sits outside it, focus is pulled inside
   * (or cleared until the scope has been laid out), so a bubbled Enter cannot
   * reach a control behind the modal.
   */
  scope?: () => { id: number } | null | undefined
}

type Placed = { id: number; x: number; y: number }

/**
 * Creates focus navigation over the `focusable` elements: spatial movement on
 * `navigate`, sequential on `cycle`, activation on `select`, consumed from an
 * input map (see FocusNavOptions.input). Spread the map's handlers on the
 * window - keys arrive there only when no focused component consumed them,
 * so a focused TextInput keeps its caret keys:
 *
 *   let nav = createFocusNav()
 *   <window {...nav.handlers}>...
 *
 * Call it inside a component/root scope: its effects and repeat timers live
 * there. move/tab/activate are the verbs, for custom triggers.
 */
let defaultMap = (): InputMap<UiActions> => {
  let map = createInputMap(uiActions)
  map.bind(uiBindings({ keyboard, gamepad: gamepad() }))
  return map
}

export function createFocusNav(options?: FocusNavOptions) {
  let currentScope = () => options?.scope?.() ?? scopeStack()[scopeStack().length - 1]

  // The currently reachable candidates with their centers: declared, laid
  // out, and inside the scope's subtree while one is set.
  let reachable = (): Placed[] => {
    let scopeNode = currentScope()
    let placed: Placed[] = []
    for (let id of getFocusables()) {
      if (scopeNode && !getNodePath(id).includes(scopeNode.id)) continue
      let b = getBoundingBoxViewport({ id })
      if (b) placed.push({ id, x: b.x + b.width / 2, y: b.y + b.height / 2 })
    }
    return placed
  }

  // Reading order: rows top to bottom (1px tie tolerance), left to right
  // within a row.
  let ordered = (placed: Placed[]): Placed[] =>
    [...placed].sort((a, b) => (Math.abs(a.y - b.y) <= 1 ? a.x - b.x : a.y - b.y))

  // Where focus last sat (a candidate's center). Navigation that finds
  // nothing focused resumes at the nearest candidate instead of restarting
  // in reading order: activating a control that is then replaced in place
  // (the dev card's Disconnect swapping to Connect) destroys the focused
  // node and clears focus, and the next press should land on the successor,
  // not the top-left of the screen.
  let lastPos: { x: number; y: number } | null = null

  let focusCandidate = (p: Placed) => {
    lastPos = { x: p.x, y: p.y }
    setFocus(p.id)
  }

  // Entry focus with no history (very first press) or where predictability
  // beats continuity (a modal opening): the first element in reading order.
  let focusFirst = (placed: Placed[]) => {
    focusCandidate(ordered(placed)[0]!)
  }

  let focusEntry = (placed: Placed[]) => {
    if (!lastPos) return focusFirst(placed)
    let { x, y } = lastPos
    let best = placed.reduce((a, b) =>
      (b.x - x) ** 2 + (b.y - y) ** 2 < (a.x - x) ** 2 + (a.y - y) ** 2 ? b : a,
    )
    focusCandidate(best)
  }

  let move = (dir: Direction) => {
    let placed = reachable()
    if (placed.length === 0) return
    let focused = focusedNode()
    let from = focused != null ? placed.find((p) => p.id === focused) : undefined
    if (!from) return focusEntry(placed)
    let best: Placed | null = null
    let bestScore = Infinity
    for (let p of placed) {
      if (p === from) continue
      let dx = p.x - from.x
      let dy = p.y - from.y
      // Progress along the pressed direction is required; among candidates the
      // nearest mostly-aligned one wins (cross-axis distance weighs double).
      let ahead = dir === "up" ? -dy : dir === "down" ? dy : dir === "left" ? -dx : dx
      if (ahead <= 1) continue
      let across = Math.abs(dir === "up" || dir === "down" ? dx : dy)
      let score = ahead + 2 * across
      if (score < bestScore) {
        bestScore = score
        best = p
      }
    }
    if (best) focusCandidate(best)
  }

  // Sequential step in reading order, wrapping at the ends. With no
  // reachable focus it resumes near the last position; lacking one, a
  // forward step enters at the first element and a backward one at the
  // last (the step "wraps into" the set from either side).
  let tab = (delta: 1 | -1) => {
    let placed = reachable()
    if (placed.length === 0) return
    let row = ordered(placed)
    let focused = focusedNode()
    let i = focused != null ? row.findIndex((p) => p.id === focused) : -1
    if (i < 0) {
      if (lastPos) return focusEntry(placed)
      return focusCandidate(row[delta === 1 ? 0 : row.length - 1]!)
    }
    focusCandidate(row[(i + delta + row.length) % row.length]!)
  }

  // The `select` press (Enter, the remote's center key, the pad's south
  // button, whatever the app bound): runs the focused node's registered
  // action; with no reachable focus the press lands focus instead of acting.
  let activate = () => {
    let placed = reachable()
    if (placed.length === 0) return
    let focused = focusedNode()
    let hit = focused != null ? placed.find((p) => p.id === focused) : undefined
    if (!hit) return focusEntry(placed)
    // Refresh the resume position before acting: the action may replace the
    // control (and take the focus) with it.
    lastPos = { x: hit.x, y: hit.y }
    navActions.get(hit.id)?.()
  }

  // The focused control vanishing (replaced by its own action, a screen
  // change) clears focus; hand it to the nearest successor so the ring
  // never disappears mid-navigation. Only when the node actually died: a
  // deliberate blur (outside tap, keyboard dismissal) leaves focus empty,
  // and the two are told apart by whether the previous node still resolves
  // (a destroyed node is gone from the tree by effect time - empty path).
  // The landing waits for the next layout: the successor was mounted this
  // very tick, so it has no box until the frame the swap itself scheduled.
  let prevFocused: number | null = null
  let refocusPending = false
  createEffect(
    () => focusedNode(),
    (id) => {
      let prev = prevFocused
      prevFocused = id
      if (id != null || prev == null) return
      refocusPending = getNodePath(prev).length === 0
    },
  )
  onLayout(() => {
    if (!refocusPending) return
    refocusPending = false
    if (focusedNode() != null) return
    let placed = reachable()
    if (placed.length > 0) focusEntry(placed)
  })

  // A scope arriving (modal opening) pulls focus inside it: focus left on an
  // outside control would still receive Enter directly (bubbling), bypassing
  // the trap. A scope mounted this very tick has no boxes yet - then focus
  // just clears, and the first navigation press lands inside.
  createEffect(
    () => currentScope(),
    (scopeNode) => {
      if (!scopeNode) return
      let focused = focusedNode()
      if (focused != null && getNodePath(focused).includes(scopeNode.id)) return
      let placed = reachable()
      if (placed.length > 0) focusFirst(placed)
      else if (focused != null) setFocus(null)
    },
  )

  // The map: the app's, checked for the UI actions, or the platform default.
  let input: InputMap<any> = options?.input ?? defaultMap()
  for (let [name, kind] of Object.entries(uiActions)) {
    if (input.actions[name] !== kind) throw new Error(`createFocusNav: the input map needs a ${kind} action "${name}" (declare it with uiActions)`)
  }
  // The direction an action reads now, or null: the dominant axis of
  // `navigate` past the threshold, the sign of `cycle`.
  let direction = (): Direction | null => {
    let [x, y] = input.value("navigate") as Vec2
    if (Math.max(Math.abs(x), Math.abs(y)) < NAV_THRESHOLD) return null
    if (Math.abs(x) > Math.abs(y)) return x > 0 ? "right" : "left"
    return y > 0 ? "down" : "up"
  }
  let cycleStep = (): 1 | -1 | null => {
    let v = input.value("cycle") as number
    return v >= NAV_THRESHOLD ? 1 : v <= -NAV_THRESHOLD ? -1 : null
  }
  repeating(direction, move)
  repeating(cycleStep, tab)
  onCleanup(input.onPress("select", activate))

  return { input, handlers: input.handlers, move, tab, activate }
}
