import { Show, children } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import { typeStyle, typeWeight, lightOnDark } from "./typography"
import { policy } from "./policy"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export type BadgeVariant = "primary" | "neutral" | "danger"

export interface BadgeProps extends TransitionProps {
  // A string/number renders as the themed pill label; anything else is rendered
  // as-is (an icon, a dot, ...).
  children?: any
  // Visual role: primary (accent), neutral (subtle surface), danger.
  variant?: BadgeVariant
  layout?: LayoutProps
  style?: StyleProps
}

// A rounded radius large enough to fully pill any typical badge height; the
// renderer clamps it to half the box, so both ends stay round.
const RADIUS = 999

// A small rounded pill for counts, labels, and status. Accent fill with
// onPrimary text by default; override the fill via style.backgroundColor and the
// label color via style.color.
export function Badge(props: BadgeProps) {
  let colors = () => {
    let c = theme.color
    switch (props.variant ?? "primary") {
      case "neutral":
        return { bg: c.surfaceAlt, fg: c.text }
      case "danger":
        return { bg: c.danger, fg: c.onPrimary }
      default:
        return { bg: c.primary, fg: c.onPrimary }
    }
  }
  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.badge, ...props.style })
  let bg = () => styled().backgroundColor ?? colors().bg
  let fg = () => styled().color ?? colors().fg
  let radius = () => styled().borderRadius ?? RADIUS
  // Resolved once via children(): the typeof probe and the mount sites must
  // share one build - reading the raw getter again would orphan native nodes.
  let resolved = children(() => props.children)
  let isText = () => typeof resolved() === "string" || typeof resolved() === "number"
  let labelOnDark = () => lightOnDark(fg(), bg())

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      flexDirection="row"
      alignItems="center"
      justifyContent="center"
      paddingLeft={8}
      paddingRight={8}
      paddingTop={2}
      paddingBottom={2}
      {...props.layout}
      x={styled().x}
      y={styled().y}
      scale={styled().scale}
      rotate={styled().rotate}
      opacity={styled().opacity}
    >
      <d-rect transition={split().background} onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)} color={bg()} radius={radius()} />
      <Show when={isText()} fallback={resolved()}>
        <text
          color={fg()}
          {...typeStyle("caption", labelOnDark())}
          fontWeight={typeWeight(600, theme.text.caption.size * policy.textScale, labelOnDark())}
        >
          {resolved()}
        </text>
      </Show>
    </view>
  )
}
