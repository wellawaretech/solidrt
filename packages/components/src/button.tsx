import { Show, children } from "@solidrt/core"
import { createPress, type PressState } from "./press"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle, lightOnDark } from "./typography"
import { Spinner } from "./spinner"
import type { LayoutProps } from "@solidrt/core"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger"
export type ButtonSize = "sm" | "md" | "lg"

export interface ButtonProps extends TransitionProps {
  // A string/number is rendered as the themed label; anything else is rendered
  // as-is, so a button can hold custom content (an icon, a row, ...).
  children?: any
  // Visual role: primary (accent fill), secondary (darker-blue fill), ghost
  // (no fill until hover), danger (destructive accent fill). None draw a border.
  variant?: ButtonVariant
  // Fixed-width preset: pins the button to a set width (a longer label still
  // expands past it), so a row of buttons lines up. Omitted, the button
  // stretches to the container's width (the default). Padding is the same at
  // every size.
  size?: ButtonSize
  // A returned promise makes this an async action: the button shows a
  // centered spinner in place of the label (geometry unchanged) and ignores
  // presses until it settles. Non-thenable returns are ignored.
  onPress?: () => unknown
  disabled?: boolean
  // Focus-navigation candidacy (spatial nav, TV remotes); on by default.
  // Disabled buttons are never candidates.
  focusable?: boolean
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
}

// Minimum width per size preset (logical px). A minimum, not a hard width, so
// labels wider than the preset never clip.
const SIZE_WIDTH: Record<ButtonSize, number> = { sm: 88, md: 120, lg: 160 }

// A themed press target: a padded, centered, accent-colored box with a label.
// Press feedback is a slight scale, hover feedback the theme's overlayHover
// tint drawn over the fill (non-touch interaction policies only), both
// reactive reads of the press state so no nodes are recreated. Override the
// box via style and the padding/sizing via layout; because hover is an
// overlay, it composes over a caller-set backgroundColor too. When disabled,
// it takes no pointer events at all. Focus (spatial nav) draws a ring under
// the focusRing policy in the theme's ring color; Enter/Space/remote-select
// activates (handled by createPress).
export function Button(props: ButtonProps) {
  // Fill and label color per variant, read reactively from the theme. No
  // variant draws a border.
  let colors = () => {
    let c = theme.color
    switch (props.variant ?? "primary") {
      case "secondary":
        return { fill: c.secondary, label: c.onSecondary }
      case "ghost":
        return { fill: "transparent", label: c.text }
      case "danger":
        return { fill: c.danger, label: c.onPrimary }
      default:
        return { fill: c.primary, label: c.onPrimary }
    }
  }
  // Theme-level per-component overrides merged under the instance style.
  let styled = (): StyleProps => ({ ...theme.components.button, ...props.style })
  let idleFill = () =>
    props.disabled
      ? props.variant === "ghost"
        ? "transparent"
        : theme.color.surface
      : colors().fill
  let bg = () => styled().backgroundColor ?? idleFill()
  // The hover feedback: the theme's overlay tint drawn over the fill, so it
  // composes with any backgroundColor (variant, theme override, or caller).
  let overlay = (s: PressState) =>
    s.hovered && !props.disabled && policy.interaction !== "touch" ? theme.color.overlayHover : "transparent"
  let radius = () => styled().borderRadius ?? theme.radius.md
  let label = () => (props.disabled ? theme.color.textMuted : colors().label)
  // Resolved once via children(): reading the raw children getter builds a new
  // subtree per read, so the typeof probe and the two mount sites below must
  // share this single memoized build (an unmounted build leaks native nodes).
  let resolved = children(() => props.children)
  let isText = () => typeof resolved() === "string" || typeof resolved() === "number"
  // The label's polarity against the idle fill: onPrimary on a saturated fill
  // is light-on-dark even in a light theme, so it needs the low-DPI weight
  // compensation there too.
  let labelOnDark = () => lightOnDark(label(), bg())

  // props (not a literal) so a swapped-in onPress is read at event time.
  let press = createPress(props)
  let style = () => ({
    ...styled(),
    ...(press.focused() && policy.focusRing ? { borderWidth: theme.borderWidth.focus, borderColor: theme.color.ring } : {}),
    backgroundColor: bg(),
    borderRadius: radius(),
    // Always a number: a scale that flips from a number back to undefined
    // hits the transform decoder, which rejects null. Multiply so a
    // caller-set scale is preserved under the press feedback.
    scale: (styled().scale ?? 1) * (press.pressed() && policy.motion !== "none" ? 0.97 : 1),
  })

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => {
        press.ref(n)
        props.ref?.(n)
      }}
      repaintBoundary
      flexDirection="row"
      alignItems="center"
      justifyContent="center"
      position="relative"
      paddingTop={space("md")}
      paddingBottom={space("md")}
      paddingLeft={space("lg")}
      paddingRight={space("lg")}
      {...(props.size ? { minWidth: SIZE_WIDTH[props.size] } : { width: "100%" })}
      {...props.layout}
      x={style().x}
      y={style().y}
      scale={style().scale}
      rotate={style().rotate}
      opacity={style().opacity}
      {...press.handlers}
      focusable={(props.focusable ?? true) && props.disabled !== true}
      pointerEvents={props.disabled ? "none" : undefined}
    >
      <d-rect transition={split().background} onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)} color={style().backgroundColor ?? "transparent"} radius={style().borderRadius} />
      <d-rect color={overlay(press.state())} radius={style().borderRadius} />
      <Show when={isText()} fallback={resolved()}>
        <text color={press.pending() ? "transparent" : label()} {...typeStyle("body", labelOnDark())}>
          {resolved()}
        </text>
      </Show>
      <Show when={press.pending()}>
        <view position="absolute" top={0} bottom={0} left={0} right={0} alignItems="center" justifyContent="center">
          <Spinner size={16} thickness={2} style={{ color: label() }} />
        </view>
      </Show>
      <Show when={(style().borderWidth ?? 0) > 0}>
        <d-rect
          drawStyle="stroke"
          transition={split().border}
          onTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
          color={style().borderColor ?? "transparent"}
          strokeWidth={style().borderWidth}
          radius={style().borderRadius}
        />
      </Show>
    </view>
  )
}
