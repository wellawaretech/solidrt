import { Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import { space } from "./spacing"
import { typeStyle } from "./typography"

export interface FieldProps {
  // Label above the control.
  label?: string
  // Help text under the control; replaced by `error` while one is set.
  description?: string
  // Validation message: rendered in the danger color in place of the
  // description.
  error?: string
  // The control (TextInput, Select, Slider, ...), rendered as-is.
  children?: any
  layout?: LayoutProps
}

// A form row: label above, control, help or error line below. It draws no
// chrome and does not reach into the control - error styling of the input
// itself stays the input's style prop (no hidden magic). The message line
// only occupies space while there is one, so forms do not jump on the first
// keystroke unless an error appears; reserve the space with a constant
// `description` if that matters.
export function Field(props: FieldProps) {
  return (
    <view flexDirection="column" gap={space("sm")} {...props.layout}>
      <Show when={props.label != null}>
        <text color={theme.color.text} {...typeStyle("label")}>
          {props.label}
        </text>
      </Show>
      {props.children}
      <Show when={props.error != null || props.description != null}>
        <text
          color={props.error != null ? theme.color.danger : theme.color.textMuted}
          {...typeStyle("caption")}
        >
          {props.error ?? props.description}
        </text>
      </Show>
    </view>
  )
}
