import { createSignal } from "@solidjs/signals"
import type { LayoutProps } from "@solidrt/core"
import { Pressable } from "./pressable"
import { theme } from "./theme"
import type { StyleProps } from "./types"

export interface SwitchProps {
  // Controlled on/off. If omitted, the switch is uncontrolled.
  value?: boolean
  // Initial value for uncontrolled use.
  defaultValue?: boolean
  onChange?: (value: boolean) => void
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

const W = 44
const H = 24
const PAD = 2
const THUMB = H - PAD * 2

// A toggle. Track fills with primary when on, surfaceAlt when off; the thumb
// slides across. Controlled via value/onChange, or uncontrolled via
// defaultValue. Built on Pressable, so disabled takes no pointer events.
export function Switch(props: SwitchProps) {
  let [internal, setInternal] = createSignal(props.defaultValue ?? false)
  let on = () => (props.value !== undefined ? props.value : internal())

  let toggle = () => {
    let next = !on()
    if (props.value === undefined) setInternal(next)
    props.onChange?.(next)
  }

  return (
    <Pressable
      onPress={toggle}
      disabled={props.disabled}
      layout={{ width: W, height: H, ...props.layout }}
      style={{
        backgroundColor: on() ? theme.color.primary : theme.color.surfaceAlt,
        borderRadius: H / 2,
        ...props.style,
      }}
    >
      <view position="absolute" top={PAD} left={PAD} x={on() ? W - THUMB - PAD * 2 : 0}>
        <d-oval w={THUMB} h={THUMB} color={theme.color.onPrimary} />
      </view>
    </Pressable>
  )
}
