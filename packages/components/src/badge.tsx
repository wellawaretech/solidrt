import { Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import { typeStyle } from "./typography"
import type { StyleProps } from "./types"

export type BadgeVariant = "primary" | "neutral" | "danger"

export interface BadgeProps {
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
  let bg = () => props.style?.backgroundColor ?? colors().bg
  let fg = () => props.style?.color ?? colors().fg
  let radius = () => props.style?.borderRadius ?? RADIUS
  let isText = () => typeof props.children === "string" || typeof props.children === "number"

  return (
    <view
      flexDirection="row"
      alignItems="center"
      justifyContent="center"
      paddingLeft={8}
      paddingRight={8}
      paddingTop={2}
      paddingBottom={2}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
    >
      <d-rect color={bg()} radius={radius()} />
      <Show when={isText()} fallback={props.children}>
        <text color={fg()} {...typeStyle("label")}>
          {props.children}
        </text>
      </Show>
    </view>
  )
}
