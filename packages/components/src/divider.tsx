import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor, withTransitionDefaults } from "./types"
import { colorFade } from "./motion"

export interface DividerProps extends TransitionProps {
  // Line direction. Horizontal (default) is a full-width rule; vertical is a
  // full-height rule for use inside a row.
  orientation?: "horizontal" | "vertical"
  // Line thickness in pixels.
  thickness?: number
  layout?: LayoutProps
  style?: StyleProps
}

// A thin rule in the theme border color. Stretches across its container on the
// cross axis (full width in a column, full height in a row); add margin via
// layout for spacing. Override the color via style.backgroundColor.
export function Divider(props: DividerProps) {
  let vertical = () => props.orientation === "vertical"
  let thickness = () => props.thickness ?? 1
  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.divider, ...props.style })
  let color = () => styled().backgroundColor ?? theme.color.border

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      width={vertical() ? thickness() : "auto"}
      height={vertical() ? "auto" : thickness()}
      alignSelf="stretch"
      {...props.layout}
    >
      <d-rect transition={withTransitionDefaults(split().background, colorFade())} onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)} color={color()} />
    </view>
  )
}
