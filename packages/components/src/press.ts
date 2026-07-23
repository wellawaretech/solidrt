import { createSignal, onSettled, setPointerCapture, releasePointerCapture, getBoundingBoxViewport } from "@solidrt/core"
import type { PointerEvent } from "@solidrt/core"

export type PressState = { pressed: boolean; hovered: boolean }

export interface PressOptions {
  onPress?: () => void
  onPointerDown?: (e: PointerEvent) => void
  onPointerUp?: (e: PointerEvent) => void
  onPointerMove?: (e: PointerEvent) => void
  onPointerEnter?: (e: PointerEvent) => void
  onPointerLeave?: (e: PointerEvent) => void
}

// Active press claims by pointer id, shared by every press recognizer. Pointer
// events dispatch leaf to root, so the innermost recognizer sees a down first
// and claims the pointer; recognizers further up the same bubble path find it
// claimed and fail silently (no pressed state, no onPress - ancestors keep
// hover only). This is the innermost-wins arena in its one-recognizer-kind
// form; raw pointer events keep bubbling regardless. Plain state on purpose:
// claims must be visible to outer recognizers within the same synchronous
// dispatch, before any signal flush.
let claims = new Map<number, symbol>()

// Whether a press recognizer currently owns this pointer. Lets sibling input
// skins (ScrollView's drag) avoid arming a gesture they will never get the up
// for: a captured press routes moves and the up exclusively to the winner.
// Interim surface until the pan recognizer arbitrates this properly.
export function isPressClaimed(pointerId: number): boolean {
  return claims.has(pointerId)
}

// The press state machine shared by the pressable components. onPress fires on
// a primary-button down followed by an up over the node. The winning
// recognizer captures the pointer, so the press survives leaving the node:
// while outside its window-relative bounds the pressed state clears (visual
// feedback retracts), wandering back in restores it (press retention), and
// only an up inside fires onPress. Enter/leave drive hover alone. Non-primary
// buttons (right/middle) do not start a press and are not forwarded. cancel()
// is the external-cancel hook (a future pan/scroll recognizer retracting a
// press); it ends the press without firing. Options are read at event time,
// so passing a component's reactive props object keeps handler changes live.
// The host view must attach `ref` for capture and retention bounds; without
// it the recognizer falls back to leave-cancels. Deliberately framework-
// agnostic (no theme, no styling): a candidate for promotion into core once
// the recognizer family grows (okf/plans/component-gestures.md).
export function createPress(options: PressOptions) {
  let [pressed, setPressed] = createSignal(false)
  let [hovered, setHovered] = createSignal(false)
  let token = Symbol()
  let node: { id: number } | null = null
  // The pointer this recognizer is tracking while a press is in flight, and
  // the retention state at the last move (read on up; the signal itself is
  // not readable same-dispatch because writes flush on the microtask).
  let active: number | null = null
  let inside = false

  let state = (): PressState => ({ pressed: pressed(), hovered: hovered() })
  let ref = (n: { id: number }) => {
    node = n
  }

  let within = (e: PointerEvent) => {
    let b = node && getBoundingBoxViewport(node)
    if (!b) return true
    return e.clientX >= b.x && e.clientX < b.x + b.width && e.clientY >= b.y && e.clientY < b.y + b.height
  }

  let release = () => {
    if (active != null) {
      claims.delete(active)
      releasePointerCapture(active)
      active = null
    }
  }
  let cancel = () => {
    release()
    setPressed(false)
  }

  // A press abandoned mid-flight (unmount during a drag) must not leave its
  // claim behind, or that pointer id could never press anything again.
  onSettled(() => release)

  let handlers = {
    onPointerDown: (e: PointerEvent) => {
      if (e.button != null && e.button !== 0) return
      if (active == null && !claims.has(e.pointerId)) {
        claims.set(e.pointerId, token)
        active = e.pointerId
        inside = true
        if (node) setPointerCapture(node.id, e.pointerId)
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
      // Without a node there is no capture and no retention bounds: an
      // uncaptured drag that leaves the box would otherwise strand the claim.
      if (!node && active === e.pointerId) cancel()
      options.onPointerLeave?.(e)
    },
  }

  return { pressed, hovered, state, ref, handlers, cancel }
}
