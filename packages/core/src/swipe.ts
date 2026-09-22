// The swipe recognizer: a one-pointer drag that, at the lift, has
// travelled SWIPE_MIN_DISTANCE from its down at SWIPE_MIN_VELOCITY or
// more, within SWIPE_ANGLE_TOLERANCE of one of the four axes. Discrete:
// onSwipe fires once, at the lift, with the direction and the velocity.
// Direction is the dominant axis of the VELOCITY, not of the travel (a
// finger that wandered then flicked swipes where it flicked). A drag
// that ends without qualifying gets onSwipeEnd alone, so a consumer that
// followed the finger snaps back.
//
// Built on createPan with the pan's axis derived from the allowed
// directions: a horizontal-only swipe inside a vertical ScrollView takes
// only horizontal drags, exactly as nested scrollers share drags today,
// and it steals at the pan's slop, so a pressable under it retracts. The
// pan measures the parent-frame velocity it hands the consumer; the
// thresholds are finger travel, so the swipe keeps a second tracker in
// window pixels over the events it wraps. The classifier is shared with
// the pointer feed's swipe sources. Mouse and touch alike.

import { createPan } from "./pan"
import type { PointerEvent } from "./types"
import { createVelocityTracker } from "./velocity"
import type { Velocity } from "./velocity"

export type SwipeDirection = "Left" | "Right" | "Up" | "Down"

export const SWIPE_DIRECTIONS: readonly SwipeDirection[] = ["Left", "Right", "Up", "Down"]

// Least finger travel (window px) from the down to the lift.
const SWIPE_MIN_DISTANCE = 24
// Least finger speed at the lift (window px/s): Android's usual
// minimumFlingVelocity of 50 dp/s at the density it assumes.
const SWIPE_MIN_VELOCITY = 300
// How far off an axis (degrees) the velocity may point.
const SWIPE_ANGLE_TOLERANCE = 30
const OFF_AXIS_RATIO = Math.tan((SWIPE_ANGLE_TOLERANCE * Math.PI) / 180)

/**
 * The direction a lift swipes in, or null: `finger` is the velocity in
 * window px/s, `travel` the window-px offset from the down.
 */
export function classifySwipe(finger: Velocity, travel: { dx: number; dy: number }, directions: readonly SwipeDirection[] = SWIPE_DIRECTIONS): SwipeDirection | null {
  if (Math.hypot(travel.dx, travel.dy) < SWIPE_MIN_DISTANCE) return null
  let ax = Math.abs(finger.vx)
  let ay = Math.abs(finger.vy)
  if (Math.hypot(ax, ay) < SWIPE_MIN_VELOCITY) return null
  let direction: SwipeDirection
  if (ax >= ay) {
    if (ay > ax * OFF_AXIS_RATIO) return null
    direction = finger.vx < 0 ? "Left" : "Right"
  } else {
    if (ax > ay * OFF_AXIS_RATIO) return null
    direction = finger.vy < 0 ? "Up" : "Down"
  }
  return directions.includes(direction) ? direction : null
}

export interface SwipeOptions {
  /** Which directions count; default all four. Decides the pan's axis:
   * only Left/Right takes horizontal drags, only Up/Down vertical ones. */
  directions?: SwipeDirection[]
  onSwipeStart?: () => void
  /** Movement since the previous event in the handler node's parent frame,
   * so a card follows the finger (createPan's frame rule). */
  onSwipeMove?: (dx: number, dy: number) => void
  /** The lift qualified: fired once, before onSwipeEnd. `velocity` is the
   * pan's, parent-frame px/s. */
  onSwipe?: (direction: SwipeDirection, velocity: Velocity) => void
  /** Every lift of an active swipe, qualified or not. */
  onSwipeEnd?: (velocity: Velocity) => void
}

export function createSwipe(options: SwipeOptions) {
  let directions = () => options.directions ?? SWIPE_DIRECTIONS
  let axisOf = (): "horizontal" | "vertical" | "both" => {
    let d = directions()
    let h = d.includes("Left") || d.includes("Right")
    let v = d.includes("Up") || d.includes("Down")
    return h && !v ? "horizontal" : v && !h ? "vertical" : "both"
  }
  // The down's window position and the finger's window-px velocity, over
  // every event the wrapped pan sees.
  let down: { id: number; x: number; y: number } | null = null
  // The lift's window position, set just before the pan's up runs.
  let lift: { x: number; y: number } | null = null
  let finger = createVelocityTracker()
  let pan = createPan({
    get axis() {
      return axisOf()
    },
    onPanStart: () => options.onSwipeStart?.(),
    onPanMove: (dx, dy) => options.onSwipeMove?.(dx, dy),
    onPanEnd: velocity => {
      let direction: SwipeDirection | null = null
      if (down && lift) {
        direction = classifySwipe(finger.velocity(), { dx: lift.x - down.x, dy: lift.y - down.y }, directions())
      }
      if (direction) options.onSwipe?.(direction, velocity)
      options.onSwipeEnd?.(velocity)
    },
  })

  let handlers = {
    onPointerDown: (e: PointerEvent) => {
      if (down === null && (e.button == null || e.button === 0)) {
        down = { id: e.pointerId, x: e.clientX, y: e.clientY }
        finger.reset()
        finger.push(e.clientX, e.clientY)
      }
      pan.handlers.onPointerDown(e)
    },
    onPointerMove: (e: PointerEvent) => {
      if (down && down.id === e.pointerId) finger.push(e.clientX, e.clientY)
      pan.handlers.onPointerMove(e)
    },
    onPointerUp: (e: PointerEvent) => {
      if (down && down.id === e.pointerId) lift = { x: e.clientX, y: e.clientY }
      pan.handlers.onPointerUp(e)
      if (down && down.id === e.pointerId) {
        down = null
        lift = null
      }
    },
  }
  let cancel = () => {
    pan.cancel()
    down = null
    lift = null
  }
  return { handlers, cancel }
}
