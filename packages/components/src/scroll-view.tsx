import { createPan, createScroll } from "@solidrt/core"
import type { LayoutProps, PointerProps, WheelEvent } from "@solidrt/core"
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
// There is no momentum yet; a fling stops when the finger lifts.
export function ScrollView(props: ScrollViewProps) {
  let viewport: { id: number } | undefined
  let content: { id: number } | undefined

  let scroll = createScroll(
    () => viewport,
    () => content,
    { axis: props.horizontal ? "horizontal" : "vertical" },
  )

  // Content follows the finger: it moves opposite to scroll offsets, which
  // grow toward the bottom/right.
  let pan = createPan({
    axis: props.horizontal ? "horizontal" : "vertical",
    onPanMove: (dx, dy) => scroll.scrollBy(-dx, -dy),
  })

  let onWheel = (e: WheelEvent) => {
    // A plain mouse wheel only emits deltaY. On a horizontal scroller, route that
    // vertical delta to the x axis so the wheel still scrolls it (trackpads that
    // emit deltaX take precedence).
    if (props.horizontal) scroll.scrollBy(e.deltaX || e.deltaY, 0)
    else scroll.scrollBy(e.deltaX, e.deltaY)
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
      <view
        ref={(n: { id: number }) => (viewport = n)}
        flex={1}
        overflow="hidden"
        clipRadius={props.style?.borderRadius}
        flexDirection={direction()}
        scrollX={scroll.offset().x}
        scrollY={scroll.offset().y}
        transition={split().viewport}
        onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
        {...pan.handlers}
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