import { Show, createRadialGradient, createSignal, getLayoutBox, withAlpha } from "@solidrt/core"
import type { LayoutProps, PointerEvent } from "@solidrt/core"
import { theme } from "./theme"
import { policy } from "./policy"
import { typeStyle } from "./typography"
import { space } from "./spacing"
import { Surface } from "./surface"
import type { ElevationLevel, StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"
import { colorFade } from "./motion"

export interface CardProps extends TransitionProps {
  children?: any
  // Optional heading rendered above the content.
  title?: string
  // Hover lift: under the pointer the card rises one elevation level (its
  // key shadow grows and moves out on the elevation motion, the card rises
  // LIFT_RISE px) and a faint highlight in the light's color follows the
  // pointer over it. Nothing rotates or scales, so edges and text stay on
  // the pixel grid. Hover interaction policies only; under the stock
  // presets, with no height to rise from, only the highlight shows.
  lift?: boolean
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
}

// The level a lifting card rises to.
const LIFTED: Record<ElevationLevel, ElevationLevel> = { flat: "raised", raised: "floating", floating: "overlay", overlay: "overlay" }
// Whole pixels, so the card comes to rest on the pixel grid.
const LIFT_RISE = 2
// Alpha of the light-colored highlight that follows the pointer over a
// lifted card.
const GLARE = 0.08

// A themed surface container: a padded column box with rounded corners, a
// surface face drawn by Surface (raised by default, theme.elevation.raised,
// zero under the stock presets; style.elevation changes the level), reading
// its colors from the theme so it recolors live. Borderless by default; pass
// style.borderWidth or style.borderColor to draw an outline. Override any
// paint via style, spacing/sizing via layout.
export function Card(props: CardProps) {
  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.card, ...props.style })
  let bg = () => styled().backgroundColor ?? theme.color.surface
  let radius = () => styled().borderRadius ?? theme.radius.lg
  let elevation = (): ElevationLevel => styled().elevation ?? "raised"
  let hasBorder = () => styled().borderWidth != null || styled().borderColor != null

  let node: { id: number } | undefined
  // Pointer position over the card, 0..1 per axis; null when not over it.
  let [pointer, setPointer] = createSignal<[number, number] | null>(null)
  let lifts = () => props.lift === true && policy.interaction !== "touch"
  let track = (e: PointerEvent) => {
    if (!lifts() || !node) return
    let box = getLayoutBox(node)
    if (!box || box.width === 0 || box.height === 0) return
    setPointer([e.localX / box.width, e.localY / box.height])
  }
  let glare = () => {
    let p = pointer()
    if (!p) return "transparent"
    return createRadialGradient(p[0], p[1], 0.9, [
      { offset: 0, color: withAlpha(theme.light.color, GLARE) },
      { offset: 1, color: withAlpha(theme.light.color, 0) },
    ])
  }
  // The rise: a whole-pixel translate on the elevation motion's timing,
  // only where the face has a height to rise from.
  let rises = () => lifts() && policy.motion === "normal" && theme.elevation[elevation()] > 0
  let riseTransition = () => (rises() ? { y: { duration: theme.motion.slow, bounce: 0.2 } } : undefined)

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => {
        node = n
        props.ref?.(n)
      }}
      repaintBoundary
      flexDirection="column"
      gap={space("lg")}
      padding={space("xl")}
      {...props.layout}
      x={styled().x}
      y={(styled().y ?? 0) - (rises() && pointer() ? LIFT_RISE : 0)}
      scale={styled().scale}
      rotate={styled().rotate}
      opacity={styled().opacity}
      onPointerMove={lifts() ? track : undefined}
      onPointerEnter={lifts() ? track : undefined}
      onPointerLeave={lifts() ? () => setPointer(null) : undefined}
    >
      <Surface
        role="surface"
        elevation={elevation()}
        fill={bg()}
        radius={radius()}
        material={styled().material}
        glow={styled().glow}
        liftTo={lifts() ? LIFTED[elevation()] : undefined}
        lifted={pointer() != null}
        outline={hasBorder() ? { color: styled().borderColor ?? theme.color.border, width: styled().borderWidth ?? theme.borderWidth.sm } : null}
        fillTransition={split().background}
        onFillTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
        outlineTransition={split().border}
        onOutlineTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
      />
      <Show when={lifts()}>
        <d-rect color={glare()} radius={radius()} />
      </Show>
      <Show when={props.title != null}>
        <text transition={colorFade()} color={theme.color.text} {...typeStyle("title")}>
          {props.title}
        </text>
      </Show>
      {props.children}
    </view>
  )
}
