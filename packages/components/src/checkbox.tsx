import { createSignal, Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress } from "./press"
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
// via defaultChecked. When disabled, it takes no pointer events at all.
export function Checkbox(props: CheckboxProps) {
  let [internal, setInternal] = createSignal(props.defaultChecked ?? false)
  let checked = () => (props.checked !== undefined ? props.checked : internal())

  let toggle = () => {
    let next = !checked()
    if (props.checked === undefined) setInternal(next)
    props.onChange?.(next)
  }
  let press = createPress({ onPress: toggle })

  let size = () => Math.round(SIZE * densityScale())
  // The checkmark in box-relative fractions, so it scales with the density.
  let check = () => {
    let s = size()
    return `M ${0.25 * s} ${0.5 * s} L ${0.45 * s} ${0.7 * s} L ${0.75 * s} ${0.3 * s}`
  }

  let style = () => ({
    backgroundColor: checked() ? theme.color.primary : theme.color.surface,
    borderColor: theme.color.border,
    borderWidth: theme.borderWidth.sm,
    borderRadius: theme.radius.sm,
    ...props.style,
  })

  return (
    <view
      repaintBoundary
      width={size()}
      height={size()}
      {...props.layout}
      x={style().x}
      y={style().y}
      scale={style().scale}
      rotate={style().rotate}
      opacity={style().opacity}
      {...press.handlers}
      pointerEvents={props.disabled ? "none" : undefined}
    >
      <d-rect color={style().backgroundColor ?? "transparent"} radius={style().borderRadius} />
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
      <Show when={(style().borderWidth ?? 0) > 0}>
        <d-rect
          drawStyle="stroke"
          color={style().borderColor ?? "transparent"}
          strokeWidth={style().borderWidth}
          radius={style().borderRadius}
        />
      </Show>
    </view>
  )
}
