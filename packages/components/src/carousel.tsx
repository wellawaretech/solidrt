import { children, createSignal, createSwipe, getLayoutBox, onLayout, untrack } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import type { StyleProps, TransitionProps, TransitionStyleProp, TransitionViewProp } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export interface CarouselProps extends TransitionProps<TransitionViewProp | TransitionStyleProp> {
  /** The pages, in order; each fills the box. */
  children?: any
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
  /** The page shown; controlled when given (pair it with onChange). */
  index?: number
  /** The page changed by a swipe or a snap. */
  onChange?: (index: number) => void
}

// The spring a page settles with after a swipe or a snap.
const PAGE_SPRING = { duration: 300 }

/**
 * A pager: the children are pages, one box wide each, laid side by side
 * and moved with the finger. A swipe (core's swipe recognizer) turns the
 * page; a drag that stops short snaps to the nearest one. Only horizontal
 * drags are taken, so a vertical ScrollView around or inside it keeps its
 * own. `index` controls the page; without it the carousel keeps its own,
 * reporting turns through `onChange` either way.
 */
export function Carousel(props: CarouselProps) {
  let node: { id: number } | undefined
  let [own, setOwn] = createSignal(0)
  let index = () => props.index ?? own()
  let [width, setWidth] = createSignal(0)
  let [drag, setDrag] = createSignal(0)
  let [dragging, setDragging] = createSignal(false)
  let pages = children(() => props.children)
  let count = () => pages.toArray().length

  onLayout(() => {
    let w = (node && getLayoutBox(node)?.width) || 0
    if (w !== width()) setWidth(w)
  })

  let go = (next: number) => {
    let clamped = Math.max(0, Math.min(next, count() - 1))
    if (clamped !== untrack(index)) {
      if (props.index === undefined) setOwn(clamped)
      props.onChange?.(clamped)
    }
  }
  let swiped = false
  let swipe = createSwipe({
    directions: ["Left", "Right"],
    onSwipeStart: () => {
      swiped = false
      setDragging(true)
    },
    onSwipeMove: dx => setDrag(drag() + dx),
    onSwipe: direction => {
      swiped = true
      go(index() + (direction === "Left" ? 1 : -1))
    },
    onSwipeEnd: () => {
      // Without a swipe the nearest page wins: the drag counts as pages.
      if (!swiped && width() > 0) go(Math.round(index() - drag() / width()))
      setDrag(0)
      setDragging(false)
    },
  })

  let split = () => splitTransition(props.transition)
  let x = () => -index() * width() + drag()

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
      clipRadius={props.style?.borderRadius}
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
      {/* transition before x: props apply in source order, so the spring
          is declared again before the settling write lands in the flush
          that ends the drag. */}
      <view flexDirection="row" alignItems="stretch" transition={dragging() ? null : { x: PAGE_SPRING }} x={x()}>
        {pages.toArray().map(page => (
          <view width={width()} flexShrink={0} flexDirection="column">
            {page}
          </view>
        ))}
      </view>
    </view>
  )
}
