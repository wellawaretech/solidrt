// What the discrete recognizers (long-press, double-tap, swipe) share: the
// copied point of a pointer event, and the slop that separates a tap from
// a move. The event object itself is rewritten per node while it bubbles
// (window.ts dispatchPath), so a recognizer keeps a copy, never the event.

import type { PointerEvent } from "./types"

/** A pointer position in every frame the event carried: window pixels
 * (client), the handler node's own pixels (local), and the frame its
 * x/y live in (parent). */
export type PointerPoint = {
  clientX: number
  clientY: number
  localX: number
  localY: number
  parentX: number
  parentY: number
}

export let pointAt = (e: PointerEvent): PointerPoint => ({ clientX: e.clientX, clientY: e.clientY, localX: e.localX, localY: e.localY, parentX: e.parentX, parentY: e.parentY })

// Finger travel in window pixels that turns a held or tapped pointer into
// a move: a long-press past it never fires, a tap past it is not one
// (iOS's 10 pt allowable movement).
export const TAP_SLOP = 10

/** Whether the pointer has left the slop circle around the down. */
export let pastSlop = (e: PointerEvent, down: { clientX: number; clientY: number }, slop = TAP_SLOP): boolean => {
  let dx = e.clientX - down.clientX
  let dy = e.clientY - down.clientY
  return dx * dx + dy * dy > slop * slop
}
