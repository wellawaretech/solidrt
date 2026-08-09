import { onSettled } from "@solidjs/signals"
import { on } from "srt:events"
import type { PointerEvent } from "./types"
import { arena, type ArenaOwner } from "./arena"

// Focal travel or span change in logical pixels before the transform
// activates; the same threshold createPan uses, so the two race fairly.
const SLOP = 8
// Per-frame EMA weight for the span that feeds `scale` (~50ms time constant
// at a 65Hz frame rate - see the zoom-noise note below). The pre-batching
// weight was 0.15 stepped per event at ~130Hz combined; steps now come once
// per frame, so the weight compounds: 1 - (1 - 0.15)^2.
const SPAN_SMOOTH = 0.28
// Pinch-vs-hold discrimination on the span RATE, an EMA (QUIET_SMOOTH
// weight) of the smoothed span's per-frame change. Engaging needs both a
// SLOP excursion and rate >= ENGAGE (~40px/s at 65Hz); an engaged pinch
// re-locks when rate falls under QUIET (~20px/s). Creep sits near 0.14
// px/frame, deliberate pinches at 1.5+ - the 2x gap between the two
// thresholds is the hysteresis that keeps the gate from chattering.
// All three step-based constants assume a ~65Hz frame rate (the captured
// tablet); a 120Hz panel halves per-step deltas. If that ever bites,
// convert the thresholds to px/s with performance.now() read at the
// pointerFrame terminator.
const QUIET = 0.3
const ENGAGE = 0.6
const QUIET_SMOOTH = 0.19

export type TransformDelta = {
  /** Focal-point movement since the previous frame, logical px. */
  dx: number
  dy: number
  /** Multiplicative span change since the previous frame (1 = unchanged, >1 = fingers spreading). */
  scale: number
  /** Rotation of the pointer pair since the previous frame, radians (0 with a single pointer). */
  rotation: number
  /** Current focal point in window coordinates (the clientX/clientY frame) - the zoom-about anchor. */
  x: number
  y: number
  /** How many pointers are down right now. Consumers that give one- and
   * two-finger translation different meanings (rotate vs pan) route on this;
   * dx/dy alone cannot tell them apart, both are focal movement. */
  pointers: number
}

export interface TransformOptions {
  onTransformStart?: () => void
  /** Streams one delta per frame; compose them multiplicatively (scale) / additively (dx, dy, rotation). */
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
// Measurement is per FRAME, not per event. Move events update pointer
// positions and arena state as they bubble (claims must stay visible
// synchronously within the dispatch walk), but the cross-pointer measure -
// centroid, span, angle - waits for the "pointerFrame" terminator the
// runtime emits after all of a frame's moves have dispatched. At that point
// every pointer is the same age, so the per-event scissor is gone by
// construction (measuring on each event paired one fresh position with the
// other pointers' frame-stale ones, and the span oscillated around its true
// value under perfectly smooth motion), and one delta emits per frame - the
// cadence anything painting consumes anyway. Anchors defer to the same
// terminator for the same reason: a mid-batch anchor would bake one
// mixed-age jolt into the first delta after it.
//
// Residual span noise survives batching, tuned against a captured tablet
// stream (sensor dither +-1-3px/event - and a resting pair's span WANDERS
// 28-76px over seconds, human fingers cannot hold separation, so no
// threshold alone can stay closed):
//   - a slop gate: scale is 1 until the span leaves its gesture baseline by
//     SLOP, so a pan or hold that never really pinches never zooms;
//   - a low-pass: once engaged, scale comes from an EMA of the span
//     (SPAN_SMOOTH). Zero-mean dither averages out; deliberate pinching - a
//     sustained signed drift - passes with ~50ms lag. With the scissor gone
//     this is belt-and-braces; a retirement candidate after an on-device
//     A/B against the raw batched span.
//   - a re-lock: pressed fingertips that HOLD still slowly flatten and roll
//     toward each other, which the sensor reports as a genuine ~9px/s span
//     shrink - a held pinch would zoom out forever. Deliberate pinches
//     measured 100-160px/s, an order of magnitude above creep, so when the
//     smoothed span rate stays under QUIET the gate re-locks and rebases;
//     crossing SLOP from the new base re-engages instantly. No amount of
//     batching removes creep: it is real reported motion.
// dx/dy have no gate or filter: their noise term is a fraction of a pixel.
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
  // The zoom gate and filter (see the zoom-noise note above): scale stays 1
  // until the SMOOTHED span leaves spanBase by SLOP, then streams smoothed
  // ratios for the rest of the gesture.
  let pinch = false
  let spanBase = 0
  let smoothSpan = 0
  // Warm-started at each engage so a fresh pinch cannot instantly re-lock.
  let spanRate = 0
  // Motion arrived this frame; measured and emitted at the terminator.
  let dirty = false
  // The anchor must be retaken at the terminator (activation and set-change
  // rebases): mid-batch the pointer map is mixed-age, so anchoring
  // immediately would bake a jolt into the next delta. A rebase frame
  // emits nothing.
  let rebase = false

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
    pinch = false
    dirty = false
    rebase = false
  }
  let cancel = reset
  let owner: ArenaOwner = { cancel }

  // The per-frame measure point: runs at the pointerFrame terminator, when
  // every tracked pointer's position is the same age.
  let flush = () => {
    if (!active) return
    if (rebase) {
      // Anchor from same-age positions; emits nothing - an activation or
      // set change must not produce a jump delta. Motion that arrived in
      // the same batch folds into the anchor.
      ref = measure()
      spanBase = ref.span
      smoothSpan = ref.span
      rebase = false
      dirty = false
      return
    }
    if (!dirty || !ref) return
    dirty = false
    let m = measure()
    let prevSpan = smoothSpan
    smoothSpan += (m.span - smoothSpan) * SPAN_SMOOTH
    spanRate += (Math.abs(smoothSpan - prevSpan) - spanRate) * QUIET_SMOOTH
    if (!pinch && Math.abs(smoothSpan - spanBase) >= SLOP) {
      // A slop excursion at speed is a pinch; at creep speed the base
      // just follows, so drift can wander forever without engaging.
      if (spanRate >= ENGAGE) pinch = true
      else spanBase = smoothSpan
    }
    let scale = pinch && prevSpan > 0 && smoothSpan > 0 ? smoothSpan / prevSpan : 1
    if (pinch && spanRate < QUIET) {
      // Holding, not pinching (see the re-lock note above).
      pinch = false
      spanBase = smoothSpan
    }
    let rotation = 0
    if (pointers.size >= 2) {
      rotation = m.angle - ref.angle
      if (rotation > Math.PI) rotation -= 2 * Math.PI
      else if (rotation < -Math.PI) rotation += 2 * Math.PI
    }
    let dx = m.x - ref.x
    let dy = m.y - ref.y
    ref = m
    options.onTransformMove?.({ dx, dy, scale, rotation, x: m.x, y: m.y, pointers: pointers.size })
  }

  // An unmount mid-gesture must not leave resolved claims behind (or a live
  // terminator subscription).
  onSettled(() => {
    let unsub = on("pointerFrame", flush)
    return () => {
      unsub()
      reset()
    }
  })

  let handlers = {
    onPointerDown: (e: PointerEvent) => {
      if (e.button != null && e.button !== 0) return
      if (pointers.has(e.pointerId)) return
      if (active) {
        // A finger joining an established gesture belongs to it outright; if
        // the arena refuses (resolved elsewhere) the finger stays out.
        if (!arena.steal(e.pointerId, owner)) return
        pointers.set(e.pointerId, { x: e.clientX, y: e.clientY })
        rebase = true
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
      if (!active) {
        // Arming runs per event: the slop test tolerates a mixed-age
        // measure (8px against <=1 frame of staleness), and the arena
        // steal must happen synchronously inside the dispatch walk.
        let m = measure()
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
        // A span-driven activation IS a deliberate pinch - engaging the
        // zoom gate now avoids demanding a second slop-crossing from it.
        pinch = Math.abs(m.span - ref.span) >= SLOP
        spanRate = pinch ? 1 : 0
        rebase = true
        options.onTransformStart?.()
        return
      }
      dirty = true
    },
    onPointerUp: (e: PointerEvent) => {
      if (!pointers.has(e.pointerId)) return
      if (active) {
        arena.release(e.pointerId, owner)
        pointers.delete(e.pointerId)
        if (pointers.size === 0) {
          active = false
          ref = null
          pinch = false
          dirty = false
          rebase = false
          options.onTransformEnd?.()
        } else {
          rebase = true
          pinch = pinch && pointers.size >= 2
          spanRate = 1
        }
        return
      }
      pointers.delete(e.pointerId)
      ref = pointers.size > 0 ? measure() : null
    },
  }

  return { handlers, cancel }
}
