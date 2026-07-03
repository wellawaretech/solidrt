import { createSignal, onCleanup, createPortal, onLayout, getBoundingBox, Show, env } from "@solidrt/core"
import type { LayoutProps, PointerEvent } from "@solidrt/core"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"

export interface TooltipProps {
  // The tooltip body. A string/number renders as themed text; anything else
  // renders as-is.
  content?: any
  // The anchor content the tooltip attaches to.
  children?: any
  // Hover delay in milliseconds before showing.
  delay?: number
  // Which side of the anchor the bubble appears on.
  placement?: "top" | "bottom"
  layout?: LayoutProps
}

const DELAY = 500
const GAP = 6
// Minimum distance kept between the bubble and the window edges.
const MARGIN = 4

/**
 * A hover-only affordance: under desktop/hybrid interaction policies, resting a
 * mouse pointer on the wrapped content shows a bubble near it after a short
 * delay. Under the touch policy it never shows, so tooltip content must stay
 * non-essential. The bubble is portal-mounted at the window root and takes no
 * pointer events; it hides on leave and on press.
 */
export function Tooltip(props: TooltipProps) {
  let anchor: { id: number } | undefined
  let [open, setOpen] = createSignal(false)
  let timer: ReturnType<typeof setTimeout> | undefined

  let enter = (e: PointerEvent) => {
    // The policy gates the behavior; the pointerType check keeps a finger from
    // arming the tooltip in a hybrid session.
    if (policy.interaction === "touch" || e.pointerType !== "mouse") return
    clearTimeout(timer)
    timer = setTimeout(() => setOpen(true), props.delay ?? DELAY)
  }
  let hide = () => {
    clearTimeout(timer)
    setOpen(false)
  }
  onCleanup(() => clearTimeout(timer))

  // The bubble sits at the window root's origin and is placed with the x/y
  // paint transforms, so repositioning never reflows. Measured after its first
  // layout (parked offscreen until then), then pinned to the anchor's current
  // box; recomputed each layout so it stays attached when the anchor moves
  // (scrolling, resizes).
  let Bubble = () => {
    let bubble: { id: number } | undefined
    let [pos, setPos] = createSignal<{ x: number; y: number } | null>(null)
    onLayout(() => {
      let a = anchor && getBoundingBox(anchor)
      let b = bubble && getBoundingBox(bubble)
      if (!a || !b) return
      let x = a.x + a.width / 2 - b.width / 2
      x = Math.round(Math.min(Math.max(x, MARGIN), env.windowSize.width - b.width - MARGIN))
      let y = Math.round(props.placement === "bottom" ? a.y + a.height + GAP : a.y - b.height - GAP)
      let cur = pos()
      if (!cur || cur.x !== x || cur.y !== y) setPos({ x, y })
    })
    let isText = () => typeof props.content === "string" || typeof props.content === "number"
    return createPortal(
      <view
        ref={(n: { id: number }) => (bubble = n)}
        repaintBoundary
        position="absolute"
        top={0}
        left={0}
        x={pos()?.x ?? -10000}
        y={pos()?.y ?? 0}
        paddingTop={space("sm")}
        paddingBottom={space("sm")}
        paddingLeft={space("md")}
        paddingRight={space("md")}
        pointerEvents="none"
      >
        <d-rect color={theme.color.surfaceAlt} radius={theme.radius.sm} />
        <Show when={isText()} fallback={props.content}>
          <text color={theme.color.text} {...typeStyle("body")}>
            {props.content}
          </text>
        </Show>
        <d-rect
          drawStyle="stroke"
          color={theme.color.border}
          strokeWidth={theme.borderWidth.sm}
          radius={theme.radius.sm}
        />
      </view>,
    )
  }

  return (
    <view
      ref={(n: { id: number }) => (anchor = n)}
      onPointerEnter={enter}
      onPointerLeave={hide}
      onPointerDown={hide}
      {...props.layout}
    >
      {props.children}
      <Show when={open()}>
        <Bubble />
      </Show>
    </view>
  )
}
