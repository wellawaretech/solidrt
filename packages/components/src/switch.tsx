import { createSignal } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress } from "./press"
import { pill, theme } from "./theme"
import { policy } from "./policy"
import { densityScale } from "./density"
import { Surface, surfaceSinks } from "./surface"
import type { ElevationLevel, StyleProps, TransitionProps, TransitionStyleProp, TransitionViewProp } from "./types"
import { partTransition, partTransitionEnd, splitTransition, transitionEndFor, withTransitionDefaults } from "./types"
import { pressScale, scaleFeedback, travelMotion } from "./motion"

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

// A toggle. The track is a sunken face, accent when on (primary, with the
// accent material's glow), control when off (surfaceAlt), fading between
// them; the thumb (theme.color.thumb, else onPrimary) is a raised control
// face that springs across - the `knob` transition entry retimes it - and
// sinks under the finger where its material has depth. Track and thumb take
// pill(): round by default, square under radius.full 0. A press shrinks the
// control slightly (pressScale) while the thumb is flat; style.elevation
// sets the thumb's level. Controlled via value/onChange, or uncontrolled
// via defaultValue. When disabled, it takes no pointer events at all.
export function Switch(props: SwitchProps) {
  let [internal, setInternal] = createSignal(props.defaultValue ?? false)
  let on = () => (props.value !== undefined ? props.value : internal())

  let toggle = () => {
    let next = !on()
    if (props.value === undefined) setInternal(next)
    props.onChange?.(next)
  }
  let press = createPress({ onPress: toggle })
  // Widens the glow; hover counts only where a pointer can hover (the
  // tint's rule).
  let active = () => press.pressed() || (press.hovered() && policy.interaction !== "touch")

  let w = () => Math.round(W * densityScale())
  let h = () => Math.round(H * densityScale())
  let thumb = () => h() - PAD * 2

  let style = () => ({
    backgroundColor: on() ? theme.color.primary : theme.color.surfaceAlt,
    borderRadius: pill(h()),
    ...theme.components.switch,
    ...props.style,
    ...(press.focused() && policy.focusRing ? { borderWidth: theme.borderWidth.focus, borderColor: theme.color.ring } : {}),
  })
  let thumbElevation = (): ElevationLevel => style().elevation ?? "raised"
  let sinks = () => surfaceSinks("control", thumbElevation(), style().material)

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
      scale={(style().scale ?? 1) * (sinks() ? 1 : pressScale(press.pressed()))}
      rotate={style().rotate}
      opacity={style().opacity}
      {...press.handlers}
      cursor="pointer"
      focusable={!props.disabled}
      pointerEvents={props.disabled ? "none" : undefined}
    >
      <Surface
        role={on() ? "accent" : "control"}
        fill={style().backgroundColor ?? "transparent"}
        radius={style().borderRadius}
        material={style().material}
        glow={on() && !props.disabled ? style().glow : null}
        sunken
        active={active()}
        outline={(style().borderWidth ?? 0) > 0 ? { color: style().borderColor ?? "transparent", width: style().borderWidth! } : null}
        fillTransition={split().background}
        onFillTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
        outlineTransition={split().border}
        onOutlineTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
      />
      <view
        position="absolute"
        top={PAD}
        left={PAD}
        width={thumb()}
        height={thumb()}
        x={on() ? w() - thumb() - PAD * 2 : 0}
        transition={partTransition(props.transition, "knob", "x", travelMotion())}
        onTransitionEnd={partTransitionEnd("knob", "x", props.onTransitionEnd)}
      >
        {/* The hairline keeps the thumb visible off: onPrimary on the
            light scheme's surfaceAlt track is about 1.2:1 on its own. */}
        <Surface
          role="control"
          elevation={thumbElevation()}
          fill={theme.color.thumb ?? theme.color.onPrimary}
          radius={pill(thumb())}
          material={style().material}
          glow={null}
          pressed={press.pressed()}
          tint={false}
          outline={{ color: theme.color.border, width: theme.borderWidth.sm }}
        />
      </view>
    </view>
  )
}
