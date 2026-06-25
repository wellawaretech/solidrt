import { Show } from "@solidrt/core"
import { Pressable, type PressState } from "./pressable"
import { theme } from "./theme"
import type { LayoutProps } from "@solidrt/core"
import type { StyleProps } from "./types"

export interface ButtonProps {
  // A string/number is rendered as the themed label; anything else is rendered
  // as-is, so a button can hold custom content (an icon, a row, ...).
  children?: any
  onPress?: () => void
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

// Themed convenience over Pressable: a padded, centered, accent-colored box with
// a label. Press feedback is a slight scale, driven through Pressable's reactive
// style so no nodes are recreated on press. Override the box via style and the
// padding/sizing via layout.
export function Button(props: ButtonProps) {
  let bg = () => props.style?.backgroundColor ?? (props.disabled ? theme.color.surface : theme.color.primary)
  let radius = () => props.style?.borderRadius ?? theme.radius.sm
  let label = () => (props.disabled ? theme.color.textMuted : theme.color.onPrimary)
  let isText = () => typeof props.children === "string" || typeof props.children === "number"

  return (
    <Pressable
      onPress={props.onPress}
      disabled={props.disabled}
      layout={{
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        paddingTop: theme.spacing.sm,
        paddingBottom: theme.spacing.sm,
        paddingLeft: theme.spacing.md,
        paddingRight: theme.spacing.md,
        ...props.layout,
      }}
      style={(s: PressState) => ({
        ...props.style,
        backgroundColor: bg(),
        borderRadius: radius(),
        // Always a number: a scale that flips from a number back to undefined
        // hits the transform decoder, which rejects null. Multiply so a
        // caller-set scale is preserved under the press feedback.
        scale: (props.style?.scale ?? 1) * (s.pressed ? 0.97 : 1),
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