import { Show, children } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import { policy } from "./policy"
import { Surface } from "./surface"
import type { ElevationLevel, MaterialRole, StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"
import { colorFade } from "./motion"

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

// A small rounded pill for counts, labels, and status. An accent face with
// onPrimary text by default (the neutral variant is a surface tone), drawn
// by Surface, flat: a badge sits in text and in list rows, so it casts
// nothing unless style.elevation raises it. Override the fill via
// style.backgroundColor, the label color via style.color, and box it with
// borderColor/borderWidth.
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
  let role = (): MaterialRole => ((props.variant ?? "primary") === "neutral" ? "surface" : "accent")
  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.badge, ...props.style })
  let bg = () => styled().backgroundColor ?? colors().bg
  let fg = () => styled().color ?? colors().fg
  let radius = () => styled().borderRadius ?? theme.radius.full
  let elevation = (): ElevationLevel => styled().elevation ?? "flat"
  let hasBorder = () => styled().borderWidth != null || styled().borderColor != null
  // Resolved once via children(): the typeof probe and the mount sites must
  // share one build - reading the raw getter again would orphan native nodes.
  let resolved = children(() => props.children)
  let isText = () => typeof resolved() === "string" || typeof resolved() === "number"

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      flexDirection="row"
      alignItems="center"
      justifyContent="center"
      paddingLeft={space("md")}
      paddingRight={space("md")}
      paddingTop={Math.round(space("sm") / 2)}
      paddingBottom={Math.round(space("sm") / 2)}
      {...props.layout}
      x={styled().x}
      y={styled().y}
      scale={styled().scale}
      rotate={styled().rotate}
      opacity={styled().opacity}
    >
      <Surface
        role={role()}
        elevation={elevation()}
        fill={bg()}
        radius={radius()}
        material={styled().material}
        glow={styled().glow}
        outline={hasBorder() ? { color: styled().borderColor ?? theme.color.border, width: styled().borderWidth ?? theme.borderWidth.sm } : null}
        fillTransition={split().background}
        onFillTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
        outlineTransition={split().border}
        onOutlineTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
      />
      <Show when={isText()} fallback={resolved()}>
        <text
          transition={colorFade()}
          color={fg()}
          {...typeStyle("caption")}
          fontWeight={600}
        >
          {resolved()}
        </text>
      </Show>
    </view>
  )
}
