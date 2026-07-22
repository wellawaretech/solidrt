import { Show, children } from "@solidrt/core"
import { Pressable, type PressState } from "./pressable"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle, lightOnDark } from "./typography"
import type { LayoutProps } from "@solidrt/core"
import type { StyleProps } from "./types"

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger"
export type ButtonSize = "sm" | "md" | "lg"

export interface ButtonProps {
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
  onPress?: () => void
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

// Minimum width per size preset (logical px). A minimum, not a hard width, so
// labels wider than the preset never clip.
const SIZE_WIDTH: Record<ButtonSize, number> = { sm: 88, md: 120, lg: 160 }

// Themed convenience over Pressable: a padded, centered, accent-colored box with
// a label. Press feedback is a slight scale, hover feedback a tint (non-touch
// interaction policies only), both driven through Pressable's reactive style so
// no nodes are recreated. Override the box via style and the padding/sizing via
// layout. A caller-set backgroundColor disables the hover tint: we cannot know
// its hover variant.
export function Button(props: ButtonProps) {
  // Fill, hover fill, and label color per variant, read reactively from the
  // theme. No variant draws a border.
  let colors = () => {
    let c = theme.color
    switch (props.variant ?? "primary") {
      case "secondary":
        return { fill: c.secondary, hover: c.secondaryHover, label: c.onSecondary }
      case "ghost":
        return { fill: "transparent", hover: c.surfaceHover, label: c.text }
      case "danger":
        return { fill: c.danger, hover: c.dangerHover, label: c.onPrimary }
      default:
        return { fill: c.primary, hover: c.primaryHover, label: c.onPrimary }
    }
  }
  let idleFill = () =>
    props.disabled
      ? props.variant === "ghost"
        ? "transparent"
        : theme.color.surface
      : colors().fill
  let bg = (s: PressState) =>
    props.style?.backgroundColor ??
    (props.disabled
      ? idleFill()
      : s.hovered && policy.interaction !== "touch"
        ? colors().hover
        : colors().fill)
  let radius = () => props.style?.borderRadius ?? theme.radius.md
  let label = () => (props.disabled ? theme.color.textMuted : colors().label)
  // Resolved once via children(): reading the raw children getter builds a new
  // subtree per read, so the typeof probe and the two mount sites below must
  // share this single memoized build (an unmounted build leaks native nodes).
  let resolved = children(() => props.children)
  let isText = () => typeof resolved() === "string" || typeof resolved() === "number"
  // The label's polarity against the idle fill: onPrimary on a saturated fill
  // is light-on-dark even in a light theme, so it needs the low-DPI weight
  // compensation there too.
  let labelOnDark = () => lightOnDark(label(), props.style?.backgroundColor ?? idleFill())

  return (
    <Pressable
      onPress={props.onPress}
      disabled={props.disabled}
      layout={{
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        paddingTop: space("md"),
        paddingBottom: space("md"),
        paddingLeft: space("lg"),
        paddingRight: space("lg"),
        ...(props.size ? { minWidth: SIZE_WIDTH[props.size] } : { width: "100%" }),
        ...props.layout,
      }}
      style={(s: PressState) => ({
        ...props.style,
        backgroundColor: bg(s),
        borderRadius: radius(),
        // Always a number: a scale that flips from a number back to undefined
        // hits the transform decoder, which rejects null. Multiply so a
        // caller-set scale is preserved under the press feedback.
        scale: (props.style?.scale ?? 1) * (s.pressed && policy.motion !== "none" ? 0.97 : 1),
      })}
    >
      <Show when={isText()} fallback={resolved()}>
        <text color={label()} {...typeStyle("body", labelOnDark())}>
          {resolved()}
        </text>
      </Show>
    </Pressable>
  )
}
