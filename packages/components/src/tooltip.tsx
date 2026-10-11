import { createSignal, onCleanup, createPortal, onLayout, getBoundingBox, Show, children } from "@solidrt/core"
import type { LayoutProps, PointerEvent } from "@solidrt/core"
import { placePopup } from "./popup"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import type { TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"
import { colorFade, popupFadeOut } from "./motion"
import { Surface } from "./surface"

export interface TooltipProps extends TransitionProps {
  // The tooltip body. A string/number renders as themed text; anything else
  // renders as-is.
  content?: any
  // The anchor content the tooltip attaches to.
  children?: any
  // Hover delay in milliseconds before showing.
  delay?: number
  // Which side of the anchor the bubble prefers; the other side is taken
  // when only it has the room.
  placement?: "top" | "bottom"
  layout?: LayoutProps
}

const DELAY = 500

/**
 * A hover-only affordance: under desktop/hybrid interaction policies, resting a
 * mouse pointer on the wrapped content shows a bubble near it after a short
 * delay. Under the touch policy it never shows, so tooltip content must stay
 * non-essential. The bubble is portal-mounted at the window root and takes no
 * pointer events; it hides on leave and on press. Its bubble is an
 * overlay face, floating.
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
  // box (placePopup, centered on the anchor); recomputed each layout so it
  // stays attached when the anchor moves (scrolling, resizes).
  let Bubble = () => {
    let bubble: { id: number } | undefined
    let [pos, setPos] = createSignal<{ x: number; y: number } | null>(null)
    onLayout(() => {
      let a = anchor && getBoundingBox(anchor)
      let b = bubble && getBoundingBox(bubble)
      if (!a || !b) return
      let p = placePopup(a, b, { side: props.placement ?? "top", align: "center" })
      let cur = pos()
      if (!cur || cur.x !== p.x || cur.y !== p.y) setPos({ x: p.x, y: p.y })
    })
    // Resolved once via children(): the typeof probe and the mount sites must
    // share one build - reading the raw getter again would orphan native nodes.
    let content = children(() => props.content)
    let isText = () => typeof content() === "string" || typeof content() === "number"
    // The fade is driven by the position write, not a mount-time `from`: the
    // bubble parks offscreen until measured, so a `from` would play unseen;
    // the exit fade plays when the bubble unmounts.
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
        opacity={pos() ? 1 : 0}
        transition={popupFadeOut()}
      >
        <Surface
          role="overlay"
          elevation={theme.components.tooltip?.elevation ?? "floating"}
          fill={theme.components.tooltip?.backgroundColor ?? theme.color.surfaceAlt}
          radius={theme.components.tooltip?.borderRadius ?? theme.radius.sm}
          material={theme.components.tooltip?.material}
          glow={theme.components.tooltip?.glow}
          outline={{
            color: theme.components.tooltip?.borderColor ?? theme.color.border,
            width: theme.components.tooltip?.borderWidth ?? theme.borderWidth.sm,
          }}
          fillTransition={split().background}
          onFillTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
          outlineTransition={split().border}
          onOutlineTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
        />
        <Show when={isText()} fallback={content()}>
          <text transition={colorFade()} color={theme.color.text} {...typeStyle("body")}>
            {content()}
          </text>
        </Show>
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
