import { onSettled } from "@solidjs/signals"
import type { PointerEvent } from "./types"
import { arena, type ArenaOwner } from "./arena"

// Focal travel or span change in logical pixels before the transform
// activates; the same threshold createPan uses, so the two race fairly.
const SLOP = 8

export type TransformDelta = {
  /** Focal-point movement since the previous event, logical px. */
  dx: number
  dy: number
  /** Multiplicative span change since the previous event (1 = unchanged, >1 = fingers spreading). */
  scale: number
  /** Rotation of the pointer pair since the previous event, radians (0 with a single pointer). */
  rotation: number
  /** Current focal point in window coordinates (the clientX/clientY frame) - the zoom-about anchor. */
  x: number
  y: number
}

export interface TransformOptions {
  onTransformStart?: () => void
  /** Streams per-event deltas; compose them multiplicatively (scale) / additively (dx, dy, rotation). */
  onTransformMove?: (t: TransformDelta) => void
  onTransformEnd?: () => void
}

// The merged transform recognizer: pan + pinch + rotate as ONE gesture over
// the set of pointers down on the node (Flutter's Scale model). Splitting
// them into separate recognizers would make them fight in the arena over the
// same fingers, so the genuinely-simultaneous case (drag while pinching a
// photo, one-finger orbit vs two-finger zoom) is a single recognizer that
// streams focal-point translation, span scale, and pair rotation together;
// consumers use the components they care about (an orbit camera reads dx/dy
// and scale, ignores rotation).
//
// Pointers arm silently on their down (no arena claim); when focal travel or
// span change from the armed configuration crosses the slop, that is positive
// evidence and the recognizer steals EVERY tracked pointer, all or nothing:
// if any is already resolved elsewhere (an inner pan won that finger) the
// whole gesture stands down. The slop is swallowed - deltas stream from the
// activation configuration on. With one finger it degrades to a plain pan
// (scale 1, rotation 0), so a drag on the node gets arena arbitration too.
//
// Set changes rebase: a finger joining (stolen outright - the gesture is
// already established) or lifting mid-gesture re-anchors the reference
// configuration, so the change itself never emits a jump delta. The gesture
// ends when the last finger lifts. Moves and ups arrive on the frozen down
// path, so an active transform survives leaving the node or the window.
// cancel() is the external-cancel hook; it ends an active gesture without
// onTransformEnd. Options are read at event time.
export function createTransform(options: TransformOptions) {
  // All tracked pointers, in down order (the first two define the rotation pair).
  let pointers = new Map<number, { x: number; y: number }>()
  let active = false
  // Slop baseline while armed; last delivered configuration while active.
  let ref: { x: number; y: number; span: number; angle: number } | null = null

  let measure = () => {
    let n = pointers.size
    let x = 0
    let y = 0
    for (let p of pointers.values()) {
      x += p.x
      y += p.y
    }
    x /= n
    y /= n
    let span = 0
    for (let p of pointers.values()) span += Math.hypot(p.x - x, p.y - y)
    span /= n
    let angle = 0
    if (n >= 2) {
      let pair = pointers.values()
      let a = pair.next().value!
      let b = pair.next().value!
      angle = Math.atan2(b.y - a.y, b.x - a.x)
    }
    return { x, y, span, angle }
  }

  let reset = () => {
    if (active) for (let id of pointers.keys()) arena.release(id, owner)
    pointers.clear()
    active = false
    ref = null
  }
  let cancel = reset
  let owner: ArenaOwner = { cancel }

  // An unmount mid-gesture must not leave resolved claims behind.
  onSettled(() => reset)

  let handlers = {
    onPointerDown: (e: PointerEvent) => {
      if (e.button != null && e.button !== 0) return
      if (pointers.has(e.pointerId)) return
      if (active) {
        // A finger joining an established gesture belongs to it outright; if
        // the arena refuses (resolved elsewhere) the finger stays out.
        if (!arena.steal(e.pointerId, owner)) return
        pointers.set(e.pointerId, { x: e.clientX, y: e.clientY })
        ref = measure()
        return
      }
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY })
      ref = measure()
    },
    onPointerMove: (e: PointerEvent) => {
      let p = pointers.get(e.pointerId)
      if (!p || !ref) return
      p.x = e.clientX
      p.y = e.clientY
      let m = measure()
      if (!active) {
        let travel = Math.hypot(m.x - ref.x, m.y - ref.y)
        if (travel < SLOP && Math.abs(m.span - ref.span) < SLOP) return
        let taken: number[] = []
        let refused = false
        for (let id of pointers.keys()) {
          if (arena.steal(id, owner)) taken.push(id)
          else {
            refused = true
            break
          }
        }
        if (refused) {
          // The gesture belongs elsewhere; hand back what we took and disarm.
          for (let id of taken) arena.release(id, owner)
          pointers.clear()
          ref = null
          return
        }
        active = true
        ref = m
        options.onTransformStart?.()
        return
      }
      let scale = ref.span > 0 && m.span > 0 ? m.span / ref.span : 1
      let rotation = 0
      if (pointers.size >= 2) {
        rotation = m.angle - ref.angle
        if (rotation > Math.PI) rotation -= 2 * Math.PI
        else if (rotation < -Math.PI) rotation += 2 * Math.PI
      }
      let dx = m.x - ref.x
      let dy = m.y - ref.y
      ref = m
      options.onTransformMove?.({ dx, dy, scale, rotation, x: m.x, y: m.y })
    },
    onPointerUp: (e: PointerEvent) => {
      if (!pointers.has(e.pointerId)) return
      if (active) {
        arena.release(e.pointerId, owner)
        pointers.delete(e.pointerId)
        if (pointers.size === 0) {
          active = false
          ref = null
          options.onTransformEnd?.()
        } else {
          ref = measure()
        }
        return
      }
      pointers.delete(e.pointerId)
      ref = pointers.size > 0 ? measure() : null
    },
  }

  return { handlers, cancel }
}
