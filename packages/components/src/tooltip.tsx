import { createSignal, onCleanup, createPortal, onLayout, getBoundingBox, Show, env, children } from "@solidrt/core"
import type { LayoutProps, PointerEvent } from "@solidrt/core"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import type { TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export interface TooltipProps extends TransitionProps {
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
// Distance between the anchor and the bubble.
let gap = () => theme.spacing.sm
// Minimum distance kept between the bubble and the window edges.
let margin = () => theme.spacing.sm

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
      x = Math.round(Math.min(Math.max(x, margin()), env.windowSize.width - b.width - margin()))
      let y = Math.round(props.placement === "bottom" ? a.y + a.height + gap() : a.y - b.height - gap())
      let cur = pos()
      if (!cur || cur.x !== x || cur.y !== y) setPos({ x, y })
    })
    // Resolved once via children(): the typeof probe and the mount sites must
    // share one build - reading the raw getter again would orphan native nodes.
    let content = children(() => props.content)
    let isText = () => typeof content() === "string" || typeof content() === "number"
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
        <d-rect
          transition={split().background}
          onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
          color={theme.components.tooltip?.backgroundColor ?? theme.color.surfaceAlt}
          radius={theme.components.tooltip?.borderRadius ?? theme.radius.sm}
        />
        <Show when={isText()} fallback={content()}>
          <text color={theme.color.text} {...typeStyle("body")}>
            {content()}
          </text>
        </Show>
        <d-rect
          drawStyle="stroke"
          transition={split().border}
          onTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
          color={theme.components.tooltip?.borderColor ?? theme.color.border}
          strokeWidth={theme.components.tooltip?.borderWidth ?? theme.borderWidth.sm}
          radius={theme.components.tooltip?.borderRadius ?? theme.radius.sm}
        />
      </view>,
    )
  }

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
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
