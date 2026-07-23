import { createSignal, For } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { Pressable, type PressState } from "./pressable"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle, lightOnDark } from "./typography"
import type { Option, StyleProps } from "./types"

export interface SegmentedControlProps {
  options: Option[]
  // Controlled selected value. If omitted, the control is uncontrolled.
  value?: unknown
  defaultValue?: unknown
  onChange?: (value: unknown) => void
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

// Width of the hairline divider between segments (logical px). The dividers
// are the backing rect showing through the row gap, so this is a gap, not a
// stroke.
const DIVIDER = 0

// A single-choice row of equal-width segments, joined flush (the Material
// style): only the control's outermost corners are rounded, interior segments
// are square, and hairline dividers separate them. The active segment fills
// with the primary color. Controlled via value/onChange, or uncontrolled via
// defaultValue. Hover tints inactive segments (non-touch interaction policies
// only). Override the inactive fill via style, spacing/sizing via layout.
export function SegmentedControl(props: SegmentedControlProps) {
  let [internal, setInternal] = createSignal(props.defaultValue)
  let value = () => (props.value !== undefined ? props.value : internal())
  let select = (v: unknown) => {
    if (props.value === undefined) setInternal(() => v)
    props.onChange?.(v)
  }

  let radius = () =>
    typeof props.style?.borderRadius === "number" ? props.style.borderRadius : theme.radius.md
  // Per-corner radii: round only the corners on the control's outer edge, so
  // the segments read as one joined control. [tl, tr, br, bl].
  let corners = (i: number): number | [number, number, number, number] => {
    let r = radius()
    let last = props.options.length - 1
    if (last === 0) return r
    if (i === 0) return [r, 0, 0, r]
    if (i === last) return [0, r, r, 0]
    return 0
  }

  let idleFill = () => props.style?.backgroundColor ?? theme.color.surfaceAlt
  let activeFill = () => (props.disabled ? theme.color.surface : theme.color.primary)
  let label = (active: boolean) =>
    props.disabled ? theme.color.textMuted : active ? theme.color.onPrimary : theme.color.text

  return (
    <view
      flexDirection="row"
      gap={DIVIDER}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      opacity={props.style?.opacity}
    >
      <d-rect color={theme.color.border} radius={radius()} />
      <For each={props.options}>
        {(opt, i) => {
          let active = () => value() === opt.value
          return (
            <Pressable
              onPress={() => select(opt.value)}
              disabled={props.disabled}
              layout={{
                flexGrow: 1,
                flexBasis: 0,
                alignItems: "center",
                paddingTop: space("md"),
                paddingBottom: space("md"),
                paddingLeft: space("md"),
                paddingRight: space("md"),
              }}
              style={(s: PressState) => ({
                backgroundColor: active()
                  ? activeFill()
                  : s.hovered && !props.disabled && policy.interaction !== "touch"
                    ? theme.color.surfaceHover
                    : idleFill(),
                borderRadius: corners(i()),
              })}
            >
              <text
                color={label(active())}
                {...typeStyle("body", active() ? lightOnDark(label(true), activeFill()) : undefined)}
              >
                {opt.label}
              </text>
            </Pressable>
          )
        }}
      </For>
    </view>
  )
}
