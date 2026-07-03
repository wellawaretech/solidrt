import { Show } from "@solidrt/core"
import { Pressable, type PressState } from "./pressable"
import { theme } from "./theme"
import { policy, densityScale } from "./policy"
import type { LayoutProps } from "@solidrt/core"
import type { StyleProps } from "./types"

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger"

export interface ButtonProps {
  // A string/number is rendered as the themed label; anything else is rendered
  // as-is, so a button can hold custom content (an icon, a row, ...).
  children?: any
  // Visual role: primary (accent fill), secondary (surface fill with border),
  // ghost (no fill until hover), danger (destructive accent fill).
  variant?: ButtonVariant
  onPress?: () => void
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

// Themed convenience over Pressable: a padded, centered, accent-colored box with
// a label. Press feedback is a slight scale, hover feedback a tint (non-touch
// interaction policies only), both driven through Pressable's reactive style so
// no nodes are recreated. Override the box via style and the padding/sizing via
// layout. A caller-set backgroundColor disables the hover tint: we cannot know
// its hover variant.
export function Button(props: ButtonProps) {
  // Fill, hover fill, and label color per variant, read reactively from the
  // theme. Only secondary draws a border.
  let colors = () => {
    let c = theme.color
    switch (props.variant ?? "primary") {
      case "secondary":
        return { fill: c.surface, hover: c.surfaceHover, label: c.text, border: c.border }
      case "ghost":
        return { fill: "transparent", hover: c.surfaceHover, label: c.text, border: undefined }
      case "danger":
        return { fill: c.danger, hover: c.dangerHover, label: c.onPrimary, border: undefined }
      default:
        return { fill: c.primary, hover: c.primaryHover, label: c.onPrimary, border: undefined }
    }
  }
  let bg = (s: PressState) =>
    props.style?.backgroundColor ??
    (props.disabled
      ? props.variant === "ghost"
        ? "transparent"
        : theme.color.surface
      : s.hovered && policy.interaction !== "touch"
        ? colors().hover
        : colors().fill)
  let radius = () => props.style?.borderRadius ?? theme.radius.sm
  let label = () => (props.disabled ? theme.color.textMuted : colors().label)
  let isText = () => typeof props.children === "string" || typeof props.children === "number"

  return (
    <Pressable
      onPress={props.onPress}
      disabled={props.disabled}
      layout={{
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        paddingTop: Math.round(theme.spacing.sm * densityScale()),
        paddingBottom: Math.round(theme.spacing.sm * densityScale()),
        paddingLeft: Math.round(theme.spacing.md * densityScale()),
        paddingRight: Math.round(theme.spacing.md * densityScale()),
        ...props.layout,
      }}
      style={(s: PressState) => ({
        borderColor: colors().border,
        borderWidth: colors().border != null ? theme.borderWidth.sm : undefined,
        ...props.style,
        backgroundColor: bg(s),
        borderRadius: radius(),
        // Always a number: a scale that flips from a number back to undefined
        // hits the transform decoder, which rejects null. Multiply so a
        // caller-set scale is preserved under the press feedback.
        scale: (props.style?.scale ?? 1) * (s.pressed && policy.motion !== "none" ? 0.97 : 1),
      })}
    >
      <Show when={isText()} fallback={props.children}>
        <text
          color={label()}
          fontSize={theme.text.body.size}
          lineHeight={theme.text.body.lineHeight}
        >
          {props.children}
        </text>
      </Show>
    </Pressable>
  )
}