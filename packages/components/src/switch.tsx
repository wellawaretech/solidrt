import { createSignal, Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress } from "./press"
import { theme } from "./theme"
import { policy } from "./policy"
import { densityScale } from "./density"
import type { StyleProps, TransitionProps, TransitionStyleProp, TransitionViewProp } from "./types"
import { partTransition, partTransitionEnd, splitTransition, transitionEndFor, withTransitionDefaults } from "./types"
import { colorFade, pressScale, scaleFeedback, travelMotion } from "./motion"

export interface SwitchProps extends TransitionProps<TransitionViewProp | TransitionStyleProp | "knob"> {
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

// A toggle. Track fills with primary when on, surfaceAlt when off (a fade);
// the thumb springs across - the `knob` transition entry retimes it. A press
// shrinks the control slightly (pressScale). Controlled via value/onChange,
// or uncontrolled via defaultValue. When disabled, it takes no pointer
// events at all.
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
    ...(press.focused() && policy.focusRing ? { borderWidth: theme.borderWidth.focus, borderColor: theme.color.ring } : {}),
  })

  let split = () => splitTransition(props.transition, ["knob"])

  return (
    <view
      transition={withTransitionDefaults(split().root, scaleFeedback())}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={press.ref}
      repaintBoundary
      width={w()}
      height={h()}
      {...props.layout}
      x={style().x}
      y={style().y}
      scale={(style().scale ?? 1) * pressScale(press.pressed())}
      rotate={style().rotate}
      opacity={style().opacity}
      {...press.handlers}
      focusable={!props.disabled}
      pointerEvents={props.disabled ? "none" : undefined}
    >
      <d-rect
        transition={withTransitionDefaults(split().background, colorFade())}
        onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
        color={style().backgroundColor ?? "transparent"}
        radius={style().borderRadius}
      />
      <view
        position="absolute"
        top={PAD}
        left={PAD}
        x={on() ? w() - thumb() - PAD * 2 : 0}
        transition={partTransition(props.transition, "knob", "x", travelMotion())}
        onTransitionEnd={partTransitionEnd("knob", "x", props.onTransitionEnd)}
      >
        <d-oval transition={colorFade()} w={thumb()} h={thumb()} color={theme.color.onPrimary} />
      </view>
      <Show when={(style().borderWidth ?? 0) > 0}>
        <d-rect
          drawStyle="stroke"
          transition={withTransitionDefaults(split().border, colorFade())}
          onTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
          color={style().borderColor ?? "transparent"}
          strokeWidth={style().borderWidth}
          radius={style().borderRadius}
        />
      </Show>
    </view>
  )
}
