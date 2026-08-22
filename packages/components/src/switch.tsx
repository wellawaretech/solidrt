import { createSignal, Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress } from "./press"
import { theme } from "./theme"
import { densityScale } from "./density"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export interface SwitchProps extends TransitionProps {
  // Controlled on/off. If omitted, the switch is uncontrolled.
  value?: boolean
  // Initial value for uncontrolled use.
  defaultValue?: boolean
  onChange?: (value: boolean) => void
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

// Designed (comfortable-density) metrics; w/h below scale them by the density
// policy.
const W = 44
const H = 24
const PAD = 2

// A toggle. Track fills with primary when on, surfaceAlt when off; the thumb
// slides across. Controlled via value/onChange, or uncontrolled via
// defaultValue. When disabled, it takes no pointer events at all.
export function Switch(props: SwitchProps) {
  let [internal, setInternal] = createSignal(props.defaultValue ?? false)
  let on = () => (props.value !== undefined ? props.value : internal())

  let toggle = () => {
    let next = !on()
    if (props.value === undefined) setInternal(next)
    props.onChange?.(next)
  }
  let press = createPress({ onPress: toggle })

  let w = () => Math.round(W * densityScale())
  let h = () => Math.round(H * densityScale())
  let thumb = () => h() - PAD * 2

  let style = () => ({
    backgroundColor: on() ? theme.color.primary : theme.color.surfaceAlt,
    borderRadius: h() / 2,
    ...theme.components.switch,
    ...props.style,
  })

  return (
    <view
      transition={splitTransition(props.transition).root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={press.ref}
      repaintBoundary
      width={w()}
      height={h()}
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
      <view position="absolute" top={PAD} left={PAD} x={on() ? w() - thumb() - PAD * 2 : 0}>
        <d-oval w={thumb()} h={thumb()} color={theme.color.onPrimary} />
      </view>
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
