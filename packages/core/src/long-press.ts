// The long-press recognizer: a pointer held LONG_PRESS_MS without leaving
// TAP_SLOP of its down fires while still down, then streams moves until
// the lift (drag-after-long-press is the reorder idiom: the finger that
// held a row now carries it).
//
// Arena: the down arms silently, no claim. A claim at the down would be
// refused wherever a press already holds the pointer (the same node, or
// an inner pressable), and the wanted behaviour there is the press
// showing pressed and the long-press winning at the timer, so the steal
// happens at the timer instead: the press is cancelled (its feedback
// retracts, it never fires), an ancestor pan can no longer take the
// finger, moves stream to the consumer. A pan that crossed its slop first
// resolved the arena, so the timer's steal is refused and the recognizer
// stands down: scrolling a list of long-pressable rows scrolls. Nested
// long-presses resolve innermost-first for free: the inner timer was
// registered first in the leaf-to-root walk. Options are read at event
// time; cancel() retracts without an end; an unmount clears the timer and
// the claim. Mouse and touch alike (a held button long-presses).
//
// The hold timer itself (createHoldTimer) is shared with the pointer
// feed's longPress source, which pulses instead of stealing: the feed is
// one arena claimant (its transform) and must stay so.

import { onSettled } from "@solidjs/signals"
import type { PointerEvent } from "./types"
import { arena, type ArenaOwner } from "./arena"
import { pastSlop, pointAt, type PointerPoint } from "./gesture"

// Hold before a long-press fires: Flutter, iOS and Android's upper value.
const LONG_PRESS_MS = 500

export interface HoldTimerOptions {
  /** Read when the timer is armed. */
  ms?: () => number | undefined
  onFire: (pointerId: number, at: PointerPoint, event: { shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean; button?: number }) => void
}

/** The arm / slop / timer machine under a long-press: one pointer at a
 * time, disarmed by travel, by a lift, by a second pointer or by cancel(). */
export function createHoldTimer(options: HoldTimerOptions) {
  let armed: { id: number; at: PointerPoint; mods: { shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean; button?: number } } | null = null
  let timer: ReturnType<typeof setTimeout> | null = null
  let disarm = () => {
    if (timer !== null) clearTimeout(timer)
    timer = null
    armed = null
  }
  return {
    /** The pointer armed right now, if any. */
    armed: () => armed?.id ?? null,
    down(e: PointerEvent) {
      if (armed) {
        // A second finger is not a long-press.
        disarm()
        return
      }
      let ms = options.ms?.() ?? LONG_PRESS_MS
      armed = { id: e.pointerId, at: pointAt(e), mods: { shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, altKey: e.altKey, metaKey: e.metaKey, button: e.button } }
      timer = setTimeout(() => {
        timer = null
        let a = armed!
        armed = null
        options.onFire(a.id, a.at, a.mods)
      }, ms)
    },
    move(e: PointerEvent) {
      if (armed && armed.id === e.pointerId && pastSlop(e, armed.at)) disarm()
    },
    up(e: PointerEvent) {
      if (armed && armed.id === e.pointerId) disarm()
    },
    cancel: disarm,
  }
}

export interface LongPressOptions {
  /** Hold before the fire, default 500 ms. */
  ms?: number
  /** Fired at the timer while the pointer is still down, with the down's
   * position. */
  onLongPress?: (at: PointerPoint) => void
  /** Movement since the previous event after the fire, in the handler
   * node's parent frame (createPan's frame rule). */
  onLongPressMove?: (dx: number, dy: number) => void
  /** The lift after a fire. */
  onLongPressEnd?: () => void
}

export function createLongPress(options: LongPressOptions) {
  // The pointer the long-press won, and its last delivered parent position.
  let fired: number | null = null
  let last: { x: number; y: number } | null = null
  let hold = createHoldTimer({
    ms: () => options.ms,
    onFire: (id, at) => {
      if (!arena.steal(id, owner)) return
      fired = id
      last = { x: at.parentX, y: at.parentY }
      options.onLongPress?.(at)
    },
  })
  let cancel = () => {
    hold.cancel()
    if (fired !== null) {
      arena.release(fired, owner)
      fired = null
      last = null
    }
  }
  let owner: ArenaOwner = { cancel }
  onSettled(() => cancel)

  let handlers = {
    onPointerDown: (e: PointerEvent) => {
      if (e.button != null && e.button !== 0) return
      if (fired !== null) return
      hold.down(e)
    },
    onPointerMove: (e: PointerEvent) => {
      if (fired === e.pointerId && last) {
        options.onLongPressMove?.(e.parentX - last.x, e.parentY - last.y)
        last = { x: e.parentX, y: e.parentY }
        return
      }
      hold.move(e)
    },
    onPointerUp: (e: PointerEvent) => {
      if (fired === e.pointerId) {
        cancel()
        options.onLongPressEnd?.()
        return
      }
      hold.up(e)
    },
  }
  return { handlers, cancel }
}
