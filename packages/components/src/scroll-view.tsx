import { createPan, createScroll, createSignal, getBoundingBoxViewport, getLayoutBox, onSettled, untrack } from "@solidrt/core"
import type { LayoutProps, PointerEvent, PointerProps, Scroll, TransitionCurve, TransitionEndEvent, WheelEvent } from "@solidrt/core"
import type { StyleProps, TransitionProps, TransitionScrollProp, TransitionStyleProp, TransitionViewProp } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export interface ScrollViewProps
  extends PointerProps,
    TransitionProps<TransitionViewProp | TransitionStyleProp | TransitionScrollProp> {
  children?: any
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
  /** Scroll the horizontal axis instead of the vertical one. */
  horizontal?: boolean
  /** Receives the scroll handle (offset, range, scrollTo) for driving the view
   * from app code; scroll policies such as following a growing log are written
   * against it. Called once the component has settled, outside any reactive
   * scope, so a signal setter can be passed directly. */
  scrollRef?: (scroll: Scroll) => void
}

// A scrollable region. The outer box carries layout/style/transform and the
// optional background and border; inside it a clipping viewport (overflow
// hidden) holds a content wrapper that takes the children's natural size. The
// offset from createScroll translates the content via scrollX/scrollY. Wheel
// and drag map to scroll deltas: positive moves the content up/left, so
// dragging a finger up reveals content below (natural scrolling). The drag is
// a pan recognizer: it activates on movement slop along the scroll axis,
// stealing the pointer from a pressable the drag started on (its press
// feedback retracts), and keeps scrolling when the pointer leaves the box.
//
// Motion: the offset is written as a target and the runtime springs to it,
// so a wheel tick glides instead of jumping and a burst of ticks retargets
// one continuous motion. While a finger drags, the spring is withdrawn from
// the viewport declaration so the content tracks the finger exactly. A lift
// at speed flings: the pan's release velocity projects ONE destination
// (the distance an exponential decay covers, clamped to the range) written
// under a tween whose curve is that decay, so the runtime animates the
// whole glide and no JS runs per frame (okf/notes/app-structure-performance.md).
// A finger landing on a moving list holds it where it is: the animated
// offset is read back from the boxes and written instantly, since the
// JS-side offset is the destination, not the position. A `scrollX`/`scrollY`
// entry in the `transition` prop replaces the default spring; the fling
// tween is the component's own.
const SCROLL_SPRING = { duration: 250 }
// Momentum after a fling: velocity decay in e-foldings per second (iOS's
// normal deceleration rate of 0.998 per ms). A fling travels its release
// speed over this.
const MOMENTUM_DECAY = 2
// The curve a fling glides on: easeOutExpo, 1 - 2^(-10 t), the shape of an
// exponential decay.
const MOMENTUM_CURVE: TransitionCurve = [0.19, 1, 0.22, 1]
// The tween's length, ms: where that curve's decay rate is MOMENTUM_DECAY
// (10 ln 2 over the rate). Constant on purpose: a faster fling travels
// farther, not longer, as a decay does; the tail is sub-pixel.
const MOMENTUM_MS = Math.round(((10 * Math.LN2) / MOMENTUM_DECAY) * 1000)
// A read-back offset this close to the written one means nothing is in
// flight: no instant write.
const LIVE_EPSILON = 0.5

export function ScrollView(props: ScrollViewProps) {
  let viewport: { id: number } | undefined
  let content: { id: number } | undefined
  let [dragging, setDragging] = createSignal(false)
  // A fling's tween is declared while it runs: cleared by its end, by a
  // wheel and by the next finger.
  let [fling, setFling] = createSignal(false)

  let scroll = createScroll(
    () => viewport,
    () => content,
    { axis: props.horizontal ? "horizontal" : "vertical" },
  )
  // Handed out from onSettled rather than the body: the body is an owned
  // scope, where a signal write (an app passing its setter) is refused.
  onSettled(() => {
    untrack(() => props.scrollRef)?.(scroll)
  })

  // Content follows the finger: it moves opposite to scroll offsets, which
  // grow toward the bottom/right. The lift's velocity (parent-frame px/s,
  // zero for a rested finger) becomes the fling's destination.
  let pan = createPan({
    axis: props.horizontal ? "horizontal" : "vertical",
    onPanStart: () => setDragging(true),
    onPanMove: (dx, dy) => scroll.scrollBy({ x: -dx, y: -dy }),
    onPanEnd: v => {
      setDragging(false)
      let speed = props.horizontal ? v.vx : v.vy
      if (speed === 0) return
      let cur = scroll.offset()
      let range = scroll.range()
      let now = props.horizontal ? cur.x : cur.y
      let dest = Math.max(0, Math.min(now - speed / MOMENTUM_DECAY, props.horizontal ? range.x : range.y))
      if (dest === now) return
      setFling(true)
      scroll.scrollTo(props.horizontal ? { x: dest } : { y: dest })
    },
  })
  // A finger landing (any down, a tap included) holds a moving list where
  // it is: the animated offset, read back from the boxes (window-relative,
  // scaled back to box pixels through the untransformed layout box), is
  // written instantly, dropping the fling or spring in flight. Nothing is
  // written when nothing moves.
  let hold = (e: PointerEvent) => {
    setFling(false)
    if (viewport && content) {
      let vb = getBoundingBoxViewport(viewport)
      let cb = getBoundingBoxViewport(content)
      let lb = getLayoutBox(viewport)
      if (vb && cb && lb) {
        let scale = props.horizontal ? (lb.width > 0 ? vb.width / lb.width : 0) : lb.height > 0 ? vb.height / lb.height : 0
        if (scale > 0) {
          let live = (props.horizontal ? vb.x - cb.x : vb.y - cb.y) / scale
          let cur = scroll.offset()
          if (Math.abs(live - (props.horizontal ? cur.x : cur.y)) > LIVE_EPSILON) {
            scroll.scrollTo(props.horizontal ? { x: live, behavior: "instant" } : { y: live, behavior: "instant" })
          }
        }
      }
    }
    pan.handlers.onPointerDown(e)
  }
  let settled = (e: TransitionEndEvent) => {
    if (e.property === "scrollX" || e.property === "scrollY") setFling(false)
    transitionEndFor("root", props.onTransitionEnd)?.(e)
  }

  let onWheel = (e: WheelEvent) => {
    setFling(false)
    // A plain mouse wheel only emits deltaY. On a horizontal scroller, route that
    // vertical delta to the x axis so the wheel still scrolls it (trackpads that
    // emit deltaX take precedence).
    if (props.horizontal) scroll.scrollBy({ x: e.deltaX || e.deltaY })
    else scroll.scrollBy({ x: e.deltaX, y: e.deltaY })
  }

  // The viewport owns the scroll offset, the outer box everything else: a
  // scrollX/scrollY entry is lifted out of the root declaration so that a
  // shared `all` does not animate opacity twice (outer times viewport).
  let split = () => {
    let t = splitTransition(props.transition)
    if (t.root == null || typeof t.root === "string") return { ...t, viewport: t.root }
    let { scrollX, scrollY, ...rest } = t.root as Record<string, unknown>
    let viewport: Record<string, unknown> = {}
    if (scrollX !== undefined) viewport.scrollX = scrollX
    if (scrollY !== undefined) viewport.scrollY = scrollY
    if (rest.all !== undefined) viewport.all = rest.all
    return {
      ...t,
      root: Object.keys(rest).length ? (rest as typeof t.root) : undefined,
      viewport: Object.keys(viewport).length ? (viewport as typeof t.root) : undefined,
    }
  }
  // The viewport's declaration: the user's scroll entries over the default
  // spring, the fling tween over both while a fling runs. During a drag,
  // and while the latest programmatic write asked for no motion (scrollTo
  // behavior "instant"), the scroll entries go, and a user `all` narrows to
  // the one other property the viewport writes (clipRadius) so it cannot
  // put a spring back under the finger or the instant write.
  let viewportTransition = () => {
    let user = split().viewport
    let entries: Record<string, unknown> = typeof user === "string" ? { all: user } : { ...(user ?? {}) }
    if (dragging() || scroll.behavior() === "instant") {
      let { scrollX, scrollY, all, ...rest } = entries
      if (all !== undefined) rest.clipRadius = all
      return Object.keys(rest).length ? rest : null
    }
    if (fling()) {
      let momentum = { duration: MOMENTUM_MS, curve: MOMENTUM_CURVE }
      return { ...entries, scrollX: momentum, scrollY: momentum }
    }
    return { scrollX: SCROLL_SPRING, scrollY: SCROLL_SPRING, ...entries }
  }
  let direction = () => (props.horizontal ? "row" : "column")
  let hasBackground = () =>
    props.style?.backgroundColor != null || props.style?.borderRadius != null
  let hasBorder = () => (props.style?.borderWidth ?? 0) > 0

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={props.ref}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      opacity={props.style?.opacity}
      onPointerEnter={props.onPointerEnter}
      onPointerLeave={props.onPointerLeave}
      onPointerDown={props.onPointerDown}
      onPointerUp={props.onPointerUp}
      onPointerMove={props.onPointerMove}
      onWheel={props.onWheel}
      pointerEvents={props.pointerEvents}
    >
      {hasBackground() ? (
        <d-rect
          transition={split().background}
          onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
          color={props.style?.backgroundColor ?? "transparent"}
          radius={props.style?.borderRadius}
        />
      ) : null}
      {/* transition before scrollX/scrollY: props apply in source order, and
          an instant write needs the withdrawn declaration to land before the
          value in the same flush, or the value starts a spring anyway. */}
      <view
        ref={(n: { id: number }) => (viewport = n)}
        flex={1}
        overflow="hidden"
        clipRadius={props.style?.borderRadius}
        flexDirection={direction()}
        transition={viewportTransition()}
        onTransitionEnd={settled}
        scrollX={scroll.offset().x}
        scrollY={scroll.offset().y}
        onPointerDown={hold}
        onPointerMove={pan.handlers.onPointerMove}
        onPointerUp={pan.handlers.onPointerUp}
        onWheel={onWheel}
      >
        <view ref={(n: { id: number }) => (content = n)} flexShrink={0} flexDirection={direction()}>
          {props.children}
        </view>
      </view>
      {hasBorder() ? (
        <d-rect
          drawStyle="stroke"
          transition={split().border}
          onTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
          color={props.style?.borderColor ?? "transparent"}
          strokeWidth={props.style?.borderWidth}
          radius={props.style?.borderRadius}
        />
      ) : null}
    </view>
  )
}