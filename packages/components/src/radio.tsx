import { createSignal, createContext, useContext, Show, children } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress } from "./press"
import { pill, theme } from "./theme"
import { policy } from "./policy"
import { densityScale } from "./density"
import { typeStyle } from "./typography"
import { Surface } from "./surface"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"
import { colorFade, markMotion, pressScale, scaleFeedback } from "./motion"

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

export interface RadioProps extends TransitionProps {
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

// A single option in a RadioGroup: a ring with an inner dot when selected,
// both on pill() (round by default, square under radius.full 0); the dot is
// an accent face (Surface, with the accent material's glow; style.elevation
// raises it). The ring color fades, the dot pops in/out (markMotion), and a
// press shrinks the ring - not the whole row, so a long label never
// wobbles. A style backgroundColor or border boxes the row as a surface
// face.
export function Radio(props: RadioProps) {
  // useContext throws ContextNotFoundError if a Radio is used outside a
  // RadioGroup (default-less context), so ctx is always present here.
  let ctx = useContext(RadioContext)
  let selected = () => ctx.value() === props.value
  let disabled = () => props.disabled || ctx.disabled()
  // Resolved once via children(): the typeof probe and the mount sites must
  // share one build - reading the raw getter again would orphan native nodes.
  let resolved = children(() => props.children)
  let isText = () => typeof resolved() === "string" || typeof resolved() === "number"

  let ring = () => Math.round(RING * densityScale())
  // Inner dot inset as a fraction of the ring, so it scales with the density.
  let inset = () => ring() * 0.3

  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.radio, ...props.style })
  let press = createPress({ onPress: () => ctx.select(props.value) })
  // The circle doubles as the focus ring: ring color at the focus width.
  let focusRing = () => press.focused() && policy.focusRing
  let ringColor = () => (focusRing() ? theme.color.ring : selected() ? theme.color.primary : theme.color.border)
  let ringWidth = () => (focusRing() ? theme.borderWidth.focus : 2)
  let boxed = () => styled().backgroundColor != null || styled().borderRadius != null || (styled().borderWidth ?? 0) > 0

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={press.ref}
      repaintBoundary
      flexDirection="row"
      alignItems="center"
      gap={theme.spacing.md}
      {...props.layout}
      x={styled().x}
      y={styled().y}
      scale={styled().scale}
      rotate={styled().rotate}
      opacity={styled().opacity}
      {...press.handlers}
      cursor="pointer"
      focusable={!disabled()}
      pointerEvents={disabled() ? "none" : undefined}
    >
      <Show when={boxed()}>
        <Surface
          role="surface"
          elevation={styled().elevation ?? "flat"}
          fill={styled().backgroundColor ?? "transparent"}
          radius={styled().borderRadius}
          material={styled().material}
          glow={null}
          outline={(styled().borderWidth ?? 0) > 0 ? { color: styled().borderColor ?? "transparent", width: styled().borderWidth! } : null}
          fillTransition={split().background}
          onFillTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
          outlineTransition={split().border}
          onOutlineTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
        />
      </Show>
      <view
        width={ring()}
        height={ring()}
        position="relative"
        scale={pressScale(press.pressed())}
        transition={scaleFeedback()}
      >
        <d-rect drawStyle="stroke" transition={colorFade()} radius={pill(ring())} color={ringColor()} strokeWidth={ringWidth()} />
        <Show when={selected()}>
          <view
            position="absolute"
            top={0}
            bottom={0}
            left={0}
            right={0}
            opacity={1}
            scale={1}
            transition={markMotion()}
          >
            <view position="absolute" left={inset()} top={inset()} width={ring() - inset() * 2} height={ring() - inset() * 2}>
              <Surface
                role="accent"
                elevation={styled().elevation ?? "flat"}
                fill={theme.color.primary}
                radius={pill(ring() - inset() * 2)}
                material={styled().material}
                glow={disabled() ? null : styled().glow}
              />
            </view>
          </view>
        </Show>
      </view>
      <Show when={isText()} fallback={resolved()}>
        <text transition={colorFade()} color={theme.color.text} {...typeStyle("body")}>
          {resolved()}
        </text>
      </Show>
    </view>
  )
}
