import { Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import { typeStyle } from "./typography"
import { space } from "./spacing"
import type { StyleProps } from "./types"

export interface CardProps {
  children?: any
  // Optional heading rendered above the content.
  title?: string
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
}

// A themed surface container: a padded column box with rounded corners, reading
// its colors from the theme so it recolors live. Borderless by default; pass
// style.borderWidth or style.borderColor to draw an outline. Override any paint
// via style, spacing/sizing via layout.
export function Card(props: CardProps) {
  let bg = () => props.style?.backgroundColor ?? theme.color.surface
  let radius = () => props.style?.borderRadius ?? theme.radius.lg
  let hasBorder = () => props.style?.borderWidth != null || props.style?.borderColor != null

  return (
    <view
      ref={props.ref}
      repaintBoundary
      flexDirection="column"
      gap={space("lg")}
      padding={space("xl")}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      opacity={props.style?.opacity}
    >
      <d-rect color={bg()} radius={radius()} />
      <Show when={props.title != null}>
        <text color={theme.color.text} {...typeStyle("title")}>
          {props.title}
        </text>
      </Show>
      {props.children}
      <Show when={hasBorder()}>
        <d-rect
          drawStyle="stroke"
          color={props.style?.borderColor ?? theme.color.border}
          strokeWidth={props.style?.borderWidth ?? theme.borderWidth.sm}
          radius={radius()}
        />
      </Show>
    </view>
  )
}
