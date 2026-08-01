import {
  createEffect,
  createSignal,
  gamepads,
  getBoundingBoxViewport,
  getFocusables,
  focusedNode,
  getNodePath,
  onLayout,
  setFocus,
} from "@solidrt/core"
import type { KeyEvent } from "@solidrt/core"

// Focus navigation: the pointer-free control path, so a TV remote, a
// keyboard, or a gamepad can drive an app. Two movement types over the same
// candidates, both steering real focus (setFocus) across the elements
// declaring `focusable`:
//
// - Spatial (arrows, dpad): judged by on-screen boxes at press time - the
//   nearest candidate with progress in the pressed direction wins, so
//   registration order never matters.
// - Sequential (Tab / Shift+Tab): visual reading order - rows top to bottom,
//   left to right within a row, wrapping at the ends. Derived from the same
//   boxes rather than registration order, which reordering mounts (a <For>
//   shuffle) silently scrambles.
//
// Activation is split by source: Enter / the remote center key bubble to the
// focused node itself (createPress consumes them there), while a controller's
// south button goes through the action registry below. Purely additive to
// pointer input: nothing is focused until the first navigation press, and
// pointer presses work unchanged throughout.

type Direction = "up" | "down" | "left" | "right"

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
 * arrows/dpad, sequential on Tab/Shift+Tab. Attach the returned onKeyDown to
 * the window - keys arrive there only when no focused component consumed
 * them, so a focused TextInput keeps its caret keys:
 *
 *   let nav = createFocusNav()
 *   <window onKeyDown={nav.onKeyDown}>...
 *
 * Gamepad dpad/south edges are wired automatically (call it inside a
 * component/root scope). move/tab/activate are exposed for custom triggers.
 */
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
  // reachable focus it resumes near the last position; lacking one, Tab
  // enters at the first element and Shift+Tab at the last (the step "wraps
  // into" the set from either side).
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

  // Controller select, and Enter reaching the window (nothing focused, or the
  // focused node did not consume it): with no reachable focus the press lands
  // focus instead of acting.
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

  // Keyboard and TV remote. Arrows and Tab use key repeat (holding walks
  // through candidates); activation ignores it. The remote center key's `key`
  // is "Unidentified", so it is matched by code.
  let onKeyDown = (e: KeyEvent) => {
    if (e.key === "ArrowUp") move("up")
    else if (e.key === "ArrowDown") move("down")
    else if (e.key === "ArrowLeft") move("left")
    else if (e.key === "ArrowRight") move("right")
    else if (e.key === "Tab") tab(e.shiftKey ? -1 : 1)
    else if ((e.key === "Enter" || e.code === "Select") && !e.repeat) activate()
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

  // Gamepads: edge-detect the dpad and south button on the union of all pads'
  // pressed buttons. The sticky replay on the first read seeds the baseline
  // (a button already held at creation fires its edge once).
  let prevButtons = new Set<string>()
  createEffect(
    () => gamepads(),
    (pads) => {
      let now = new Set<string>()
      for (let pad of pads) for (let b of pad?.buttons ?? []) now.add(b)
      for (let b of now) {
        if (prevButtons.has(b)) continue
        if (b === "dpadUp") move("up")
        else if (b === "dpadDown") move("down")
        else if (b === "dpadLeft") move("left")
        else if (b === "dpadRight") move("right")
        else if (b === "south") activate()
      }
      prevButtons = now
    },
  )

  return { onKeyDown, move, tab, activate }
}
