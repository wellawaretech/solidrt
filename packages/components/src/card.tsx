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
  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.card, ...props.style })
  let bg = () => styled().backgroundColor ?? theme.color.surface
  let radius = () => styled().borderRadius ?? theme.radius.lg
  let hasBorder = () => styled().borderWidth != null || styled().borderColor != null

  return (
    <view
      ref={props.ref}
      repaintBoundary
      flexDirection="column"
      gap={space("lg")}
      padding={space("xl")}
      {...props.layout}
      x={styled().x}
      y={styled().y}
      scale={styled().scale}
      rotate={styled().rotate}
      opacity={styled().opacity}
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
          color={styled().borderColor ?? theme.color.border}
          strokeWidth={styled().borderWidth ?? theme.borderWidth.sm}
          radius={radius()}
        />
      </Show>
    </view>
  )
}
