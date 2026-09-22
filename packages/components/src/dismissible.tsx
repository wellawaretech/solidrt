import { createSignal, createSwipe, getLayoutBox } from "@solidrt/core"
import type { LayoutProps, SwipeDirection, TransitionEndEvent } from "@solidrt/core"
import type { StyleProps, TransitionProps, TransitionStyleProp, TransitionViewProp } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export type DismissDirection = "Left" | "Right"

export interface DismissibleProps extends TransitionProps<TransitionViewProp | TransitionStyleProp> {
  children?: any
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
  /** Which swipes dismiss: either way (default), or leftward or rightward
   * only. A drag the other way is not started at all. */
  direction?: "horizontal" | "left" | "right"
  /** The content has left the box, with the swipe's direction. Remove
   * the row here; until then it stays parked past the edge. */
  onDismiss?: (direction: DismissDirection) => void
}

// The leave after a qualifying swipe finishes the remaining travel at the
// swipe's own speed, within these bounds (ms): a flick never drags on, a
// slow qualifying swipe never snaps.
const LEAVE_MIN_MS = 120
const LEAVE_MAX_MS = 400
// The spring back to rest after a drag that did not qualify.
const SNAP_BACK = { duration: 250 }

/**
 * Swipe-to-dismiss: the content follows the finger sideways and, on a
 * qualifying swipe (core's swipe recognizer: enough travel and speed,
 * within 30 degrees of the axis), leaves the box that way and reports
 * `onDismiss`; a drag that stops short springs back. The recognizer steals
 * the pointer at its slop, so a pressable row still presses on a tap and
 * retracts once the drag is one; a vertical drag over the row is left to an
 * enclosing ScrollView. Mouse and touch alike. The box itself does not
 * shrink: the caller removes the row (its exit transition then plays).
 */
export function Dismissible(props: DismissibleProps) {
  let node: { id: number } | undefined
  let [offset, setOffset] = createSignal(0)
  let [dragging, setDragging] = createSignal(false)
  // The leave in flight: its tween, and the direction to report at its
  // end. Plain state is the truth (onSwipe and onSwipeEnd run in one
  // handler, before any signal flush); the signal drives the motion.
  let leave: { duration: number; direction: DismissDirection } | null = null
  let [leaving, setLeaving] = createSignal<{ duration: number; direction: DismissDirection } | null>(null)

  let directions = (): SwipeDirection[] => (props.direction === "left" ? ["Left"] : props.direction === "right" ? ["Right"] : ["Left", "Right"])
  let width = () => (node && getLayoutBox(node)?.width) || 0

  let swipe = createSwipe({
    get directions() {
      return directions()
    },
    onSwipeStart: () => setDragging(true),
    onSwipeMove: dx => setOffset(offset() + dx),
    onSwipe: (direction, velocity) => {
      let w = width()
      let target = direction === "Left" ? -w : w
      let remaining = Math.abs(target - offset())
      let speed = Math.abs(velocity.vx)
      let duration = speed > 0 ? Math.min(LEAVE_MAX_MS, Math.max(LEAVE_MIN_MS, (remaining / speed) * 1000)) : LEAVE_MAX_MS
      leave = { duration, direction: direction as DismissDirection }
      setLeaving(leave)
      setOffset(target)
    },
    onSwipeEnd: () => {
      setDragging(false)
      if (!leave) setOffset(0)
    },
  })

  let settled = (e: TransitionEndEvent) => {
    if (e.property === "x" && leave) {
      let done = leave
      leave = null
      setLeaving(null)
      props.onDismiss?.(done.direction)
    }
  }
  // The content's x: tracked exactly under the finger, tweened out on a
  // leave, sprung back otherwise.
  let motion = () => {
    if (dragging()) return null
    let leave = leaving()
    return leave ? { x: { duration: leave.duration, curve: "ease-out" as const } } : { x: SNAP_BACK }
  }
  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => {
        node = n
        props.ref?.(n)
      }}
      overflow="hidden"
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      opacity={props.style?.opacity}
      {...swipe.handlers}
    >
      {props.style?.backgroundColor != null || props.style?.borderRadius != null ? (
        <d-rect
          transition={split().background}
          onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
          color={props.style?.backgroundColor ?? "transparent"}
          radius={props.style?.borderRadius}
        />
      ) : null}
      {/* transition before x: props apply in source order, and the leave's
          tween must be declared before the value that starts it lands in
          the same flush, or the write snaps and no end ever fires. */}
      <view flexDirection="column" transition={motion()} onTransitionEnd={settled} x={offset()}>
        {props.children}
      </view>
    </view>
  )
}
