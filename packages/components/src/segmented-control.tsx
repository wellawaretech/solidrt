import { createSignal, For, Show, getBoundingBox, onCleanup, onLayout, withAlpha } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress } from "./press"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import { Surface, facePaint } from "./surface"
import type { ElevationLevel, Option, StyleProps, TransitionProps, TransitionStyleProp, TransitionViewProp } from "./types"
import { partTransition, partTransitionEnd, splitTransition, transitionEndFor } from "./types"
import { colorFade, PressFeedback, travelMotion } from "./motion"

export interface SegmentedControlProps<T = unknown>
  extends TransitionProps<TransitionViewProp | TransitionStyleProp | "indicator"> {
  options: Option<T>[]
  // Controlled selected value. If omitted, the control is uncontrolled.
  value?: T
  defaultValue?: T
  onChange?: (value: T) => void
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
// are square, and hairline dividers separate them. The track is a sunken
// control face; the active segment is an accent face drawn under the labels
// as detached rects (its fill and, standing raised, its shadows) that spring
// between segments - the `indicator` transition entry retimes it; a press on
// its segment sinks it. Controlled via value/onChange, or uncontrolled via
// defaultValue. Hover tints inactive segments (non-touch interaction
// policies only). Override the inactive fill via style, box the control
// with borderColor/borderWidth, the indicator's level with style.elevation,
// the spacing/sizing via layout.
export function SegmentedControl<T>(props: SegmentedControlProps<T>) {
  // The value overload: as far as TypeScript knows T could be a function,
  // which createSignal would read as a compute; the cast picks the value form.
  let [internal, setInternal] = createSignal<T | undefined>(props.defaultValue as Exclude<T | undefined, Function>)
  let value = () => (props.value !== undefined ? props.value : internal())
  let select = (v: T) => {
    if (props.value === undefined) setInternal(() => v)
    props.onChange?.(v)
  }

  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.segmentedControl, ...props.style })
  let radius = () => {
    let r = styled().borderRadius
    return typeof r === "number" ? r : theme.radius.md
  }
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

  let idleFill = () => styled().backgroundColor ?? theme.color.surfaceAlt
  let hasBorder = () => styled().borderWidth != null || styled().borderColor != null
  let activeFill = () => (props.disabled ? theme.color.surface : theme.color.primary)
  let elevation = (): ElevationLevel => styled().elevation ?? "raised"
  let label = (active: boolean) =>
    props.disabled ? theme.color.textMuted : active ? theme.color.onPrimary : theme.color.text

  // The indicator is positioned from measured segment boxes (control-relative
  // x and width per segment), so a selection change retargets it without a
  // reflow and unequal rounding never misaligns it. Remeasured each layout.
  // Segment nodes are keyed by option value, not by the For index: a ref
  // callback runs in the element's owned scope, where reading the index
  // signal warns (STRICT_READ_UNTRACKED) and would go stale on a reorder.
  let root: { id: number } | undefined
  let segs = new Map<T, { id: number }>()
  let [boxes, setBoxes] = createSignal<{ x: number; w: number }[]>([])
  // Which segment is pressed, so the indicator sinks under the finger on
  // its own segment only.
  let [pressedValue, setPressedValue] = createSignal<T | undefined>(undefined)
  // The indicator's travel spring is armed only after the first placement:
  // the declaration exists from mount, so without this the first measured
  // write would slide the indicator in from x 0. A timer, not the same
  // flush - the declaration must be committed strictly after that write.
  let [placed, setPlaced] = createSignal(false)
  onLayout(() => {
    if (!root) return
    let r = getBoundingBox(root)
    if (!r) return
    let next: { x: number; w: number }[] = []
    for (let i = 0; i < props.options.length; i++) {
      let s = segs.get(props.options[i]!.value)
      let b = s && getBoundingBox(s)
      if (!b) return
      next.push({ x: b.x - r.x, w: b.width })
    }
    let cur = boxes()
    if (cur.length !== next.length || cur.some((c, i) => c.x !== next[i]!.x || c.w !== next[i]!.w)) setBoxes(next)
    if (!placed()) setTimeout(() => setPlaced(true), 0)
  })
  let activeIndex = () => props.options.findIndex((o) => o.value === value())
  let indicator = () => boxes()[activeIndex()]
  let indicatorPaint = () =>
    facePaint({
      role: "accent",
      fill: activeFill(),
      elevation: elevation(),
      material: styled().material,
      glow: indicator() && !props.disabled ? styled().glow : null,
      pressed: pressedValue() !== undefined && pressedValue() === value(),
    })

  let split = () => splitTransition(props.transition, ["indicator"])
  let indicatorTransition = () => {
    if (props.transition === null) return null
    let travel = placed() ? partTransition(props.transition, "indicator", "x", travelMotion()) : undefined
    let fade = colorFade()
    if (!travel && !fade) return undefined
    return { ...fade, ...(travel as object | undefined) }
  }

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => (root = n)}
      flexDirection="row"
      gap={DIVIDER}
      {...props.layout}
      x={styled().x}
      y={styled().y}
      scale={styled().scale}
      rotate={styled().rotate}
      opacity={styled().opacity}
    >
      <Surface
        role="control"
        fill={idleFill()}
        radius={radius()}
        material={styled().material}
        glow={null}
        sunken
        outline={hasBorder() ? { color: styled().borderColor ?? theme.color.border, width: styled().borderWidth ?? theme.borderWidth.sm } : null}
        fillTransition={split().background}
        onFillTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
        outlineTransition={split().border}
        onOutlineTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
      />
      {/* The indicator's shadows ride two hidden rects in its own color under
          its fill (a rect carries one shadow; opaque casters, see Surface),
          all three on the same x/w so they travel as one. */}
      <Show when={indicatorPaint().contactShadow}>
        {(shadow) => (
          <d-rect
            transition={indicatorTransition()}
            color={indicator() ? activeFill() : withAlpha(activeFill(), 0)}
            x={indicator()?.x ?? 0}
            w={indicator()?.w ?? 0}
            radius={corners(activeIndex())}
            shadow={shadow()}
          />
        )}
      </Show>
      <Show when={indicatorPaint().keyShadow}>
        {(shadow) => (
          <d-rect
            transition={indicatorTransition()}
            color={indicator() ? activeFill() : withAlpha(activeFill(), 0)}
            x={indicator()?.x ?? 0}
            w={indicator()?.w ?? 0}
            radius={corners(activeIndex())}
            shadow={shadow()}
          />
        )}
      </Show>
      <d-rect
        transition={indicatorTransition()}
        onTransitionEnd={partTransitionEnd("indicator", "x", props.onTransitionEnd)}
        color={indicator() ? indicatorPaint().fill : withAlpha(activeFill(), 0)}
        x={indicator()?.x ?? 0}
        w={indicator()?.w ?? 0}
        radius={corners(activeIndex())}
        shadow={indicatorPaint().glow}
      />
      <Show when={indicatorPaint().bevel}>
        {(bevel) => (
          <d-rect
            transition={indicatorTransition()}
            drawStyle="stroke"
            strokeWidth={1}
            color={indicator() ? bevel() : withAlpha(activeFill(), 0)}
            x={indicator()?.x ?? 0}
            w={indicator()?.w ?? 0}
            radius={corners(activeIndex())}
          />
        )}
      </Show>
      <For each={props.options}>
        {(opt, i) => {
          let active = () => value() === opt.value
          let press = createPress({
            onPress: () => select(opt.value),
            onPointerDown: () => setPressedValue(() => opt.value),
            onPointerUp: () => setPressedValue(undefined),
            onPointerCancel: () => setPressedValue(undefined),
          })
          onCleanup(() => segs.delete(opt.value))
          return (
            <view
              ref={(n: { id: number }) => {
                segs.set(opt.value, n)
                press.ref(n)
              }}
              repaintBoundary
              flexGrow={1}
              flexBasis={0}
              alignItems="center"
              paddingTop={space("md")}
              paddingBottom={space("md")}
              paddingLeft={space("md")}
              paddingRight={space("md")}
              {...press.handlers}
              cursor="pointer"
              focusable={!props.disabled}
              pointerEvents={props.disabled ? "none" : undefined}
            >
              <PressFeedback
                pressed={press.pressed()}
                hovered={press.hovered() && !props.disabled && policy.interaction !== "touch"}
                radius={corners(i())}
              />
              <Show when={press.focused() && policy.focusRing}>
                <d-rect drawStyle="stroke" color={theme.color.ring} strokeWidth={theme.borderWidth.focus} radius={corners(i())} />
              </Show>
              <text
                transition={colorFade()}
                color={label(active())}
                {...typeStyle("body")}
              >
                {opt.label}
              </text>
            </view>
          )
        }}
      </For>
    </view>
  )
}
