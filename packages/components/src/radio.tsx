import { createSignal, createContext, useContext, Show, children } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress } from "./press"
import { theme } from "./theme"
import { densityScale } from "./policy"
import { typeStyle } from "./typography"
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
  // Resolved once via children(): the typeof probe and the mount sites must
  // share one build - reading the raw getter again would orphan native nodes.
  let resolved = children(() => props.children)
  let isText = () => typeof resolved() === "string" || typeof resolved() === "number"

  let ring = () => Math.round(RING * densityScale())
  // Inner dot inset as a fraction of the ring, so it scales with the density.
  let inset = () => ring() * 0.3

  let press = createPress({ onPress: () => ctx.select(props.value) })

  return (
    <view
      ref={press.ref}
      repaintBoundary
      flexDirection="row"
      alignItems="center"
      gap={theme.spacing.md}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      opacity={props.style?.opacity}
      {...press.handlers}
      pointerEvents={disabled() ? "none" : undefined}
    >
      <Show when={props.style?.backgroundColor != null || props.style?.borderRadius != null}>
        <d-rect color={props.style?.backgroundColor ?? "transparent"} radius={props.style?.borderRadius} />
      </Show>
      <view width={ring()} height={ring()}>
        <d-oval x={1} y={1} w={ring() - 2} h={ring() - 2} drawStyle="stroke" color={ringColor()} strokeWidth={2} />
        <Show when={selected()}>
          <d-oval x={inset()} y={inset()} w={ring() - inset() * 2} h={ring() - inset() * 2} color={theme.color.primary} />
        </Show>
      </view>
      <Show when={isText()} fallback={resolved()}>
        <text color={theme.color.text} {...typeStyle("body")}>
          {resolved()}
        </text>
      </Show>
      <Show when={(props.style?.borderWidth ?? 0) > 0}>
        <d-rect
          drawStyle="stroke"
          color={props.style?.borderColor ?? "transparent"}
          strokeWidth={props.style?.borderWidth}
          radius={props.style?.borderRadius}
        />
      </Show>
    </view>
  )
}
