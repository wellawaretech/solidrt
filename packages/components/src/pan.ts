import { onSettled } from "@solidrt/core"
import type { PointerEvent } from "@solidrt/core"
import { release, steal } from "./arena"

// Movement in logical pixels before a pan activates. Below this a drag still
// reads as a press (tap wiggle); crossing it is the positive evidence that the
// gesture is a pan, at which point the pan steals the pointer in the arena.
const PAN_SLOP = 8

export type PanAxis = "vertical" | "horizontal" | "both"

export interface PanOptions {
  /**
   * Which movement direction activates the pan. Slop is axis-aware: a
   * "vertical" pan only activates once vertical travel crosses the threshold,
   * so nested cross-axis scrollers each take only drags along their own axis.
   * Default "both" (straight-line distance).
   */
  axis?: PanAxis
  onPanStart?: () => void
  /** Pointer movement since the previous event; positive dx is rightward, positive dy downward. */
  onPanMove?: (dx: number, dy: number) => void
  onPanEnd?: () => void
}

// The pan recognizer: turns a drag into a movement-delta stream. On a down it
// arms and starts measuring; when travel from the down point crosses the slop
// along the enabled axis it activates, stealing the pointer in the arena (a
// press that provisionally owned it is cancelled - its feedback retracts) and
// resolving it so no other recognizer can take the drag over. If the arena is
// already resolved (an inner pan won first) the recognizer disarms and stays
// out. The slop distance itself is swallowed: deltas stream from the
// activation point on. Moves and the up arrive on the frozen down path, so an
// active pan keeps streaming when the pointer leaves the node or the window.
// cancel() is the external-cancel hook; it ends an active pan without
// onPanEnd. Options are read at event time. Deliberately framework-agnostic:
// a candidate for promotion into core (okf/plans/component-gestures.md).
export function createPan(options: PanOptions) {
  // Down position while armed; last delivered position while active.
  let origin: { x: number; y: number } | null = null
  let active: number | null = null
  let armed: number | null = null

  let past = (e: PointerEvent) => {
    if (!origin) return false
    let dx = Math.abs(e.clientX - origin.x)
    let dy = Math.abs(e.clientY - origin.y)
    let axis = options.axis ?? "both"
    if (axis === "vertical") return dy >= PAN_SLOP
    if (axis === "horizontal") return dx >= PAN_SLOP
    return dx * dx + dy * dy >= PAN_SLOP * PAN_SLOP
  }

  let reset = () => {
    if (active != null) {
      release(active, owner)
      active = null
    }
    armed = null
    origin = null
  }
  let cancel = reset
  let owner = { cancel }

  // An unmount mid-drag must not leave a resolved claim behind.
  onSettled(() => reset)

  let handlers = {
    onPointerDown: (e: PointerEvent) => {
      if (e.button != null && e.button !== 0) return
      if (armed == null && active == null) {
        armed = e.pointerId
        origin = { x: e.clientX, y: e.clientY }
      }
    },
    onPointerMove: (e: PointerEvent) => {
      if (armed === e.pointerId && past(e)) {
        if (steal(e.pointerId, owner)) {
          active = e.pointerId
          armed = null
          origin = { x: e.clientX, y: e.clientY }
          options.onPanStart?.()
        } else {
          // The arena is resolved against us; the drag belongs elsewhere.
          reset()
        }
        return
      }
      if (active === e.pointerId && origin) {
        options.onPanMove?.(e.clientX - origin.x, e.clientY - origin.y)
        origin = { x: e.clientX, y: e.clientY }
      }
    },
    onPointerUp: (e: PointerEvent) => {
      if (active === e.pointerId) {
        reset()
        options.onPanEnd?.()
      } else if (armed === e.pointerId) {
        reset()
      }
    },
  }

  return { handlers, cancel }
}
