import { Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import type { StyleProps } from "./types"

export interface BadgeProps {
  // A string/number renders as the themed pill label; anything else is rendered
  // as-is (an icon, a dot, ...).
  children?: any
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
  let bg = () => props.style?.backgroundColor ?? theme.color.primary
  let fg = () => props.style?.color ?? theme.color.onPrimary
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
        <text color={fg()} fontSize={12} fontWeight={600}>
          {props.children}
        </text>
      </Show>
    </view>
  )
}
