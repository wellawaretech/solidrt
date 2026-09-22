// The double-tap recognizer: two taps, the second down within
// DOUBLE_TAP_MS of the first up, within DOUBLE_TAP_SLOP of it and at
// least DOUBLE_TAP_MIN_MS after it (a bouncing contact is one tap, not
// two). Fires on the second DOWN (Flutter, iOS), so it is not delayed by
// the second lift. A tap is a down that stays within TAP_SLOP and lifts
// before the long-press hold (a longer hold is a long-press, never half a
// double-tap).
//
// Arena: the first tap claims nothing (it must not disturb a press on the
// same node) but PENDS a decision on its pointer, so a press that
// resolved at the first lift defers its firing (arena.defer): the single
// tap fires once the window passes with no second tap, and never when
// the double-tap wins. The second down steals its pointer outright (the
// press's provisional claim is cancelled, so the second tap never shows
// pressed or fires) and releases it at the lift. Options are read at
// event time; cancel() retracts without firing; an unmount clears the
// window timer. Mouse and touch alike (a double-click double-taps).
//
// The tap bookkeeping (createTapSequence) is shared with the pointer
// feed's doubleTap source, which pulses instead of stealing.

import { onSettled } from "@solidjs/signals"
import type { PointerEvent } from "./types"
import { arena, type ArenaOwner } from "./arena"
import { pastSlop, pointAt, type PointerPoint } from "./gesture"

// Second down at most this long after the first up.
const DOUBLE_TAP_MS = 300
// Second down at most this far (window px) from the first down: Flutter's
// kDoubleTapSlop, generous because the second tap of a quick pair lands
// where the hand is, not where the eye is.
const DOUBLE_TAP_SLOP = 100
// Second down at least this long after the first up (Flutter's
// kDoubleTapMinTime): sooner is a contact bounce.
const DOUBLE_TAP_MIN_MS = 40
// A down held longer than this before its lift is not a tap.
const TAP_MAX_MS = 500

type Mods = { shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean }

export interface TapSequenceOptions {
  /** A first tap started: its outcome will be decided. */
  onPend?: (pointerId: number) => void
  /** The first tap's outcome: won (a second tap followed) or lost. */
  onDecide?: (pointerId: number, won: boolean) => void
  /** The second qualifying down, with its position and modifiers. */
  onDouble: (pointerId: number, at: PointerPoint, mods: Mods) => void
}

/** The tap bookkeeping under a double-tap: one first tap at a time,
 * decided by the second down, by travel, by a long hold, by the window
 * timer or by cancel(). */
export function createTapSequence(options: TapSequenceOptions) {
  let first: { id: number; at: PointerPoint; downAt: number; upAt: number | null } | null = null
  let timer: ReturnType<typeof setTimeout> | null = null
  let settle = (won: boolean) => {
    if (!first) return
    if (timer !== null) clearTimeout(timer)
    timer = null
    let id = first.id
    first = null
    options.onDecide?.(id, won)
  }
  return {
    down(e: PointerEvent) {
      let now = performance.now()
      if (first) {
        if (first.upAt === null) {
          // A second finger while the first is down: neither is a tap.
          if (first.id !== e.pointerId) settle(false)
          return
        }
        let gap = now - first.upAt
        if (gap >= DOUBLE_TAP_MIN_MS && gap <= DOUBLE_TAP_MS && !pastSlop(e, first.at, DOUBLE_TAP_SLOP)) {
          settle(true)
          options.onDouble(e.pointerId, pointAt(e), { shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, altKey: e.altKey, metaKey: e.metaKey })
          return
        }
        // Too far, too late or a bounce: the first tap stands alone and
        // this down starts over.
        settle(false)
      }
      first = { id: e.pointerId, at: pointAt(e), downAt: now, upAt: null }
      options.onPend?.(e.pointerId)
    },
    move(e: PointerEvent) {
      if (first && first.id === e.pointerId && first.upAt === null && pastSlop(e, first.at)) settle(false)
    },
    up(e: PointerEvent) {
      if (!first || first.id !== e.pointerId || first.upAt !== null) return
      let now = performance.now()
      if (now - first.downAt > TAP_MAX_MS) {
        settle(false)
        return
      }
      first.upAt = now
      timer = setTimeout(() => {
        timer = null
        settle(false)
      }, DOUBLE_TAP_MS)
    },
    cancel: () => settle(false),
  }
}

export interface DoubleTapOptions {
  /** The second tap's down, with its position. */
  onDoubleTap?: (at: PointerPoint) => void
}

export function createDoubleTap(options: DoubleTapOptions) {
  // The second tap's pointer, held from its down to its lift.
  let second: number | null = null
  let release = () => {
    if (second !== null) {
      arena.release(second, owner)
      second = null
    }
  }
  let cancel = () => {
    taps.cancel()
    release()
  }
  let owner: ArenaOwner = { cancel }
  let taps = createTapSequence({
    onPend: id => arena.pend(id, owner),
    onDecide: (id, won) => arena.decide(id, owner, won),
    onDouble: (id, at) => {
      if (!arena.steal(id, owner)) return
      second = id
      options.onDoubleTap?.(at)
    },
  })
  onSettled(() => cancel)

  let handlers = {
    onPointerDown: (e: PointerEvent) => {
      if (e.button != null && e.button !== 0) return
      taps.down(e)
    },
    onPointerMove: (e: PointerEvent) => {
      taps.move(e)
    },
    onPointerUp: (e: PointerEvent) => {
      if (second === e.pointerId) release()
      taps.up(e)
    },
  }
  return { handlers, cancel }
}
