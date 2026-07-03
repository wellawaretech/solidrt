import { Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import { typeStyle } from "./typography"
import type { StyleProps } from "./types"

export interface CardProps {
  children?: any
  // Optional heading rendered above the content.
  title?: string
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
}

// A themed surface container: a padded column box with a subtle border and
// rounded corners, reading its colors from the theme so it recolors live.
// Override any paint via style, spacing/sizing via layout.
export function Card(props: CardProps) {
  let bg = () => props.style?.backgroundColor ?? theme.color.surface
  let border = () => props.style?.borderColor ?? theme.color.border
  let width = () => props.style?.borderWidth ?? theme.borderWidth.sm
  let radius = () => props.style?.borderRadius ?? theme.radius.lg

  return (
    <view
      ref={props.ref}
      flexDirection="column"
      gap={theme.spacing.lg}
      padding={theme.spacing.xl}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
    >
      <d-rect color={bg()} radius={radius()} />
      <Show when={props.title != null}>
        <text color={theme.color.text} {...typeStyle("title")}>
          {props.title}
        </text>
      </Show>
      {props.children}
      <d-rect drawStyle="stroke" color={border()} strokeWidth={width()} radius={radius()} />
    </view>
  )
}
