import { createSignal, createContext, useContext, Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { Pressable } from "./pressable"
import { theme } from "./theme"
import type { StyleProps } from "./types"

// Shared selection state for a group. Created and consumed within this module, so
// RadioGroup/Radio are a self-contained pair, not a cross-component dependency.
type RadioContextValue = {
  value: () => unknown
  select: (value: unknown) => void
  disabled: () => boolean
}

let RadioContext = createContext<RadioContextValue>()

export interface RadioGroupProps {
  // Controlled selected value. If omitted, the group is uncontrolled.
  value?: unknown
  defaultValue?: unknown
  onChange?: (value: unknown) => void
  disabled?: boolean
  layout?: LayoutProps
  children?: any
}

// Owns the selection for its Radio children and shares it through context.
export function RadioGroup(props: RadioGroupProps) {
  let [internal, setInternal] = createSignal(props.defaultValue)
  let value = () => (props.value !== undefined ? props.value : internal())

  let select = (v: unknown) => {
    if (props.value === undefined) setInternal(() => v)
    props.onChange?.(v)
  }

  let ctx: RadioContextValue = {
    value,
    select,
    disabled: () => !!props.disabled,
  }

  return (
    <RadioContext value={ctx}>
      <view flexDirection="column" gap={theme.spacing.md} {...props.layout}>
        {props.children}
      </view>
    </RadioContext>
  )
}

export interface RadioProps {
  // This option's value; selecting it makes it the group's value.
  value: unknown
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
  // A string/number renders as a themed label beside the ring; anything else
  // renders as-is.
  children?: any
}

const RING = 20

// A single option in a RadioGroup: a ring with an inner dot when selected.
export function Radio(props: RadioProps) {
  // useContext throws ContextNotFoundError if a Radio is used outside a
  // RadioGroup (default-less context), so ctx is always present here.
  let ctx = useContext(RadioContext)
  let selected = () => ctx.value() === props.value
  let disabled = () => props.disabled || ctx.disabled()
  let ringColor = () => (selected() ? theme.color.primary : theme.color.border)
  let isText = () => typeof props.children === "string" || typeof props.children === "number"

  return (
    <Pressable
      onPress={() => ctx.select(props.value)}
      disabled={disabled()}
      layout={{ flexDirection: "row", alignItems: "center", gap: theme.spacing.md, ...props.layout }}
      style={props.style}
    >
      <view width={RING} height={RING}>
        <d-oval x={1} y={1} w={RING - 2} h={RING - 2} drawStyle="stroke" color={ringColor()} strokeWidth={2} />
        <Show when={selected()}>
          <d-oval x={6} y={6} w={RING - 12} h={RING - 12} color={theme.color.primary} />
        </Show>
      </view>
      <Show when={isText()} fallback={props.children}>
        <text color={theme.color.text} fontSize={theme.text.body.size} lineHeight={theme.text.body.lineHeight}>
          {props.children}
        </text>
      </Show>
    </Pressable>
  )
}
