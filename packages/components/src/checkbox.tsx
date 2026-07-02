import { createSignal, Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { Pressable } from "./pressable"
import { theme } from "./theme"
import { densityScale } from "./policy"
import type { StyleProps } from "./types"

export interface CheckboxProps {
  // Controlled checked state. If omitted, the checkbox is uncontrolled.
  checked?: boolean
  defaultChecked?: boolean
  onChange?: (checked: boolean) => void
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

const SIZE = 20

// A checkbox. When checked, fills with primary and draws a checkmark; otherwise
// shows an empty bordered box. Controlled via checked/onChange, or uncontrolled
// via defaultChecked. Built on Pressable.
export function Checkbox(props: CheckboxProps) {
  let [internal, setInternal] = createSignal(props.defaultChecked ?? false)
  let checked = () => (props.checked !== undefined ? props.checked : internal())

  let toggle = () => {
    let next = !checked()
    if (props.checked === undefined) setInternal(next)
    props.onChange?.(next)
  }

  let size = () => Math.round(SIZE * densityScale())
  // The checkmark in box-relative fractions, so it scales with the density.
  let check = () => {
    let s = size()
    return `M ${0.25 * s} ${0.5 * s} L ${0.45 * s} ${0.7 * s} L ${0.75 * s} ${0.3 * s}`
  }

  return (
    <Pressable
      onPress={toggle}
      disabled={props.disabled}
      layout={{ width: size(), height: size(), ...props.layout }}
      style={{
        backgroundColor: checked() ? theme.color.primary : theme.color.surface,
        borderColor: theme.color.border,
        borderWidth: theme.borderWidth.sm,
        borderRadius: theme.radius.sm,
        ...props.style,
      }}
    >
      <Show when={checked()}>
        <d-path
          d={check()}
          drawStyle="stroke"
          color={theme.color.onPrimary}
          strokeWidth={2}
          strokeCap="round"
          strokeJoin="round"
        />
      </Show>
    </Pressable>
  )
}
