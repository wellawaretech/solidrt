import { createScroll } from "@solidrt/core"
import type { LayoutProps, PointerEvent, PointerProps, WheelEvent } from "@solidrt/core"
import { isPressClaimed } from "./press"
import type { StyleProps } from "./types"

export interface ScrollViewProps extends PointerProps {
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
// offset from createScroll translates the content via scrollX/scrollY. Wheel and
// drag map to scroll deltas: positive moves the content up/left, so dragging a
// finger up reveals content below (natural scrolling). There is no momentum yet;
// a fling stops when the finger lifts. With no pointer capture, a drag that
// leaves the box stops scrolling (pointerLeave ends the gesture).
export function ScrollView(props: ScrollViewProps) {
  let viewport: { id: number } | undefined
  let content: { id: number } | undefined

  let scroll = createScroll(
    () => viewport,
    () => content,
    { axis: props.horizontal ? "horizontal" : "vertical" },
  )

  // Last pointer position during an active drag, in window coordinates. null
  // when no drag is in progress.
  let last: { x: number; y: number } | null = null

  let onPointerDown = (e: PointerEvent) => {
    // A press-claimed pointer is captured by the pressed node: this viewport
    // would see the bubbled down but never the up, leaving the drag armed
    // forever. Until the pan recognizer can steal such a pointer on slop, a
    // drag that starts on a pressable does not scroll (the wheel still does).
    if (isPressClaimed(e.pointerId)) return
    last = { x: e.clientX, y: e.clientY }
  }
  let onPointerMove = (e: PointerEvent) => {
    if (!last) return
    scroll.scrollBy(last.x - e.clientX, last.y - e.clientY)
    last = { x: e.clientX, y: e.clientY }
  }
  let endDrag = () => {
    last = null
  }
  let onWheel = (e: WheelEvent) => {
    // A plain mouse wheel only emits deltaY. On a horizontal scroller, route that
    // vertical delta to the x axis so the wheel still scrolls it (trackpads that
    // emit deltaX take precedence).
    if (props.horizontal) scroll.scrollBy(e.deltaX || e.deltaY, 0)
    else scroll.scrollBy(e.deltaX, e.deltaY)
  }

  let direction = () => (props.horizontal ? "row" : "column")
  let hasBackground = () =>
    props.style?.backgroundColor != null || props.style?.borderRadius != null
  let hasBorder = () => (props.style?.borderWidth ?? 0) > 0

  return (
    <view
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
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerLeave={endDrag}
        onWheel={onWheel}
      >
        <view ref={(n: { id: number }) => (content = n)} flexShrink={0} flexDirection={direction()}>
          {props.children}
        </view>
      </view>
      {hasBorder() ? (
        <d-rect
          drawStyle="stroke"
          color={props.style?.borderColor ?? "transparent"}
          strokeWidth={props.style?.borderWidth}
          radius={props.style?.borderRadius}
        />
      ) : null}
    </view>
  )
}