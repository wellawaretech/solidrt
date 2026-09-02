import { createSignal, onFrame, onLayout, getBoundingBox, Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import { policy } from "./policy"
import type { StyleProps, TransitionProps, TransitionStyleProp, TransitionViewProp } from "./types"
import { partTransition, partTransitionEnd, splitTransition, transitionEndFor, withTransitionDefaults } from "./types"
import { colorFade, travelMotion } from "./motion"

export interface ProgressBarProps extends TransitionProps<TransitionViewProp | TransitionStyleProp | "fill"> {
  // Progress from 0 to 1. Omit (or leave undefined) for an indeterminate bar: a
  // segment that slides back and forth.
  value?: number
  layout?: LayoutProps
  style?: StyleProps
}

const HEIGHT = 6
// Indeterminate: fraction of the track the sliding segment occupies, and its
// travel speed in track-widths per second.
const SEGMENT = 0.3
const SPEED = 0.8

let clamp = (x: number, lo: number, hi: number) => (x < lo ? lo : x > hi ? hi : x)

// A horizontal progress bar. Determinate when given a value in [0, 1]: the fill
// grows from the left. Indeterminate when value is undefined: a short segment
// slides back and forth (driven by core onFrame). Colors come from the theme;
// override the track via style.backgroundColor and the fill via style.color.
export function ProgressBar(props: ProgressBarProps) {
  let h = () => (props.layout?.height as number) ?? HEIGHT
  let radius = () => h() / 2
  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.progressBar, ...props.style })
  let track = () => styled().backgroundColor ?? theme.color.surfaceAlt
  let fill = () => styled().color ?? theme.color.primary
  let indeterminate = () => props.value === undefined

  // Measured track width in pixels. Both the determinate fill and the
  // indeterminate segment are drawn as a detached d-rect sized in pixels (paint
  // only) rather than animating a percentage `width`, which would reflow taffy
  // whenever the value changes. Refreshed each layout so it tracks resizes.
  let trackNode: { id: number } | undefined
  let [trackWidth, setTrackWidth] = createSignal(0)
  onLayout(() => {
    if (!trackNode) return
    setTrackWidth(getBoundingBox(trackNode)?.width ?? 0)
  })

  // tick is in milliseconds (like performance.now()). The frame loop is mounted
  // through the <Show> below only while indeterminate and the motion policy
  // allows it: onFrame holds a standing frame request while registered, so a
  // check inside the callback would keep the renderer free-running (which is
  // also why a determinate bar must not register it at all). "reduced" halves
  // the travel speed; under "none" the segment parks centered (phase 0.5).
  let [phase, setPhase] = createSignal(0)
  let animating = () => indeterminate() && policy.motion !== "none"
  let Animate = () => {
    onFrame((tick) => {
      // Triangle wave in [0, 1]: the segment travels left edge -> right edge
      // and back.
      let t = ((tick / 1000) * (policy.motion === "reduced" ? SPEED / 2 : SPEED)) % 2
      setPhase(t > 1 ? 2 - t : t)
    })
    return null
  }

  // Fill width in pixels: a fixed segment while indeterminate, the value fraction
  // otherwise. Drawn via the detached d-rect `w` below (paint only), so a
  // changing value never reflows. The indeterminate slide comes from the `x`
  // offset.
  let fillWidth = () => (indeterminate() ? SEGMENT : clamp(props.value ?? 0, 0, 1)) * trackWidth()
  let effectivePhase = () => (policy.motion === "none" ? 0.5 : phase())
  let offset = () => effectivePhase() * trackWidth() * (1 - SEGMENT)

  let split = () => splitTransition(props.transition, ["fill"])
  // Determinate only: a value write glides the fill (the `fill` transition
  // entry retimes it; bounce 0, progress must not overshoot). Indeterminate
  // writes w/x per frame, which a transition would lag behind.
  let fillTransition = () => {
    if (props.transition === null) return null
    let travel = indeterminate() ? undefined : partTransition(props.transition, "fill", "w", travelMotion(0))
    let fade = colorFade()
    if (!travel && !fade) return undefined
    return { ...fade, ...(travel as object | undefined) }
  }

  return (
    <view ref={(n: { id: number }) => (trackNode = n)} position="relative" width="100%" height={h()} transition={split().root} onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)} {...props.layout}>
      <Show when={animating()}>
        <Animate />
      </Show>
      <d-rect transition={withTransitionDefaults(split().background, colorFade())} onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)} color={track()} radius={radius()} />
      <d-rect transition={fillTransition()} onTransitionEnd={partTransitionEnd("fill", "w", props.onTransitionEnd)} color={fill()} radius={radius()} w={fillWidth()} h={h()} x={indeterminate() ? offset() : 0} />
    </view>
  )
}
