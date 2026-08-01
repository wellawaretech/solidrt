import { createSignal, createMemo, onSettled, focusedNode, getBoundingBoxViewport } from "@solidrt/core"
import type { PointerEvent, KeyEvent } from "@solidrt/core"
import { claim, release } from "./arena"
import { registerNavAction } from "./focus-nav"

// A live view of a recognizer's state, not a snapshot: the fields are getters,
// so a consumer that reads one inside a JSX prop or child expression tracks that
// signal there and nothing else re-runs. Read them in those positions, not
// eagerly into a local, or the read lands in whatever scope destructured it.
export type PressState = { pressed: boolean; hovered: boolean; focused: boolean }

export interface PressOptions {
  onPress?: () => void
  disabled?: boolean
  onPointerDown?: (e: PointerEvent) => void
  onPointerUp?: (e: PointerEvent) => void
  onPointerMove?: (e: PointerEvent) => void
  onPointerEnter?: (e: PointerEvent) => void
  onPointerLeave?: (e: PointerEvent) => void
  onKeyDown?: (e: KeyEvent) => void
  onFocus?: () => void
  onBlur?: () => void
}

// The press state machine shared by the pressable components. onPress fires on
// a primary-button down followed by an up over the node. The down provisionally
// claims the pointer in the arena; pointer events dispatch leaf to root, so the
// innermost recognizer claims first and recognizers further up the same bubble
// path find the pointer taken and fail silently (no pressed state, no onPress -
// ancestors keep hover only). The claim is stealable: a pan recognizer crossing
// its slop takes the pointer and this press is cancelled through the arena.
//
// Moves and the up arrive on the frozen down path, so the press survives
// leaving the node: while outside its window-relative bounds the pressed state
// clears (visual feedback retracts), wandering back in restores it (press
// retention), and only an up inside fires onPress. Enter/leave drive hover
// alone. Non-primary buttons (right/middle) do not start a press. cancel() is
// the external-cancel hook; it ends the press without firing. Options are read
// at event time, so passing a component's reactive props object keeps handler
// changes live. The host view must attach `ref` for retention bounds; without
// it every position counts as inside (the up always fires).
//
// Keyboard/remote activation: when the host node holds focus (spatial nav or
// setFocus), Enter, Space, or the remote center key fires onPress and stops
// propagating; `focused` mirrors the node's focus for styling (a ring). The
// ref also registers onPress as the node's nav action, the path a
// controller's south button activates through (see focus-nav.ts). Key
// activation shows no pressed state - the focus ring is the feedback.
// Deliberately framework-agnostic (no theme, no styling): a candidate for
// promotion into core once the recognizer family grows
// (okf/plans/component-gestures.md).
export function createPress(options: PressOptions) {
  let [pressed, setPressed] = createSignal(false)
  let [hovered, setHovered] = createSignal(false)
  let node: { id: number } | null = null
  let unregisterNav: (() => void) | null = null

  // Focus is derived from core's reactive focus rather than tracked through
  // the onFocus/onBlur handlers - one source of truth. Memoized so a focus
  // move propagates into styling only for the two controls whose value flips.
  // focusedNode() is read FIRST, unconditionally: the memo may first compute
  // before the ref has set `node`, and short-circuiting past the read would
  // leave the memo with no dependency, frozen false forever.
  let focused = createMemo(() => {
    let id = focusedNode()
    return id != null && id === node?.id
  })
  // The pointer this recognizer is tracking while a press is in flight, and
  // the retention state at the last move (read on up; the signal itself is
  // not readable same-dispatch because writes flush on the microtask).
  let active: number | null = null
  let inside = false

  // One stable object of getters, handed out as-is. Returning a fresh snapshot
  // instead would read both signals at call time, making them dependencies of
  // the caller's scope - and for render-prop children that scope is the one
  // that builds the subtree, so a hover or press would rebuild it. A rebuild
  // mid-gesture replaces a nested recognizer with a fresh one that never saw
  // the down, so its up fires nothing: invisible with a mouse (hover settles
  // long before the click) and fatal on touch, where the finger's arrival flips
  // the ancestor's hover during the very gesture it is meant to recognize.
  let live: PressState = {
    get pressed() {
      return pressed()
    },
    get hovered() {
      return hovered()
    },
    get focused() {
      return focused()
    },
  }
  let state = (): PressState => live
  let ref = (n: { id: number }) => {
    node = n
    unregisterNav?.()
    unregisterNav = registerNavAction(n.id, () => {
      if (!options.disabled) options.onPress?.()
    })
  }

  let within = (e: PointerEvent) => {
    let b = node && getBoundingBoxViewport(node)
    if (!b) return true
    return e.clientX >= b.x && e.clientX < b.x + b.width && e.clientY >= b.y && e.clientY < b.y + b.height
  }

  let disengage = () => {
    if (active != null) {
      release(active, owner)
      active = null
    }
  }
  let cancel = () => {
    disengage()
    setPressed(false)
  }
  let owner = { cancel }

  // A press abandoned mid-flight (unmount during a drag) must not leave its
  // claim behind, or that pointer id could never press anything again.
  onSettled(() => () => {
    disengage()
    unregisterNav?.()
  })

  let handlers = {
    onPointerDown: (e: PointerEvent) => {
      if (e.button != null && e.button !== 0) return
      if (active == null && claim(e.pointerId, owner)) {
        active = e.pointerId
        inside = true
        setPressed(true)
      }
      options.onPointerDown?.(e)
    },
    onPointerMove: (e: PointerEvent) => {
      if (active === e.pointerId) {
        inside = within(e)
        setPressed(inside)
      }
      options.onPointerMove?.(e)
    },
    onPointerUp: (e: PointerEvent) => {
      if (active === e.pointerId) {
        let fire = inside
        cancel()
        if (fire) options.onPress?.()
      }
      options.onPointerUp?.(e)
    },
    onPointerEnter: (e: PointerEvent) => {
      setHovered(true)
      options.onPointerEnter?.(e)
    },
    onPointerLeave: (e: PointerEvent) => {
      setHovered(false)
      options.onPointerLeave?.(e)
    },
    onKeyDown: (e: KeyEvent) => {
      // The remote center key's `key` is "Unidentified"; match its code.
      if ((e.key === "Enter" || e.key === " " || e.code === "Select") && !e.repeat && !options.disabled) {
        e.stopPropagation()
        options.onPress?.()
      }
      options.onKeyDown?.(e)
    },
    onFocus: () => {
      options.onFocus?.()
    },
    onBlur: () => {
      options.onBlur?.()
    },
  }

  return { pressed, hovered, focused, state, ref, handlers, cancel }
}
