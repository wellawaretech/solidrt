import { createSignal, Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress } from "./press"
import { theme } from "./theme"
import { policy } from "./policy"
import { densityScale } from "./density"
import { Icon } from "./icon"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export interface CheckboxProps extends TransitionProps {
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
    ...theme.components.checkbox,
    ...props.style,
    ...(press.focused() && policy.focusRing ? { borderWidth: theme.borderWidth.focus, borderColor: theme.color.ring } : {}),
  })

  return (
    <view
      transition={splitTransition(props.transition).root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={press.ref}
      repaintBoundary
      width={size()}
      height={size()}
      position="relative"
      {...props.layout}
      x={style().x}
      y={style().y}
      scale={style().scale}
      rotate={style().rotate}
      opacity={style().opacity}
      {...press.handlers}
      focusable={!props.disabled}
      pointerEvents={props.disabled ? "none" : undefined}
    >
      <d-rect color={style().backgroundColor ?? "transparent"} radius={style().borderRadius} />
      <Show when={checked()}>
        <Show
          when={theme.icons.check}
          fallback={
            <d-path
              d={check()}
              drawStyle="stroke"
              color={theme.color.onPrimary}
              strokeWidth={2}
              strokeCap="round"
              strokeJoin="round"
            />
          }
        >
          <view position="absolute" top={0} bottom={0} left={0} right={0} alignItems="center" justifyContent="center">
            <Icon src={theme.icons.check!} size={Math.round(size() * 0.75)} color={theme.color.onPrimary} />
          </view>
        </Show>
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
