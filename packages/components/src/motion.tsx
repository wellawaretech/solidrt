import { withAlpha } from "@solidrt/core"
import type { Transition } from "@solidrt/core"
import { theme } from "./theme"
import { policy } from "./policy"

// The components' built-in motion. Every default transition in the package
// is declared through these helpers, so timing comes from theme.motion and
// the whole set degrades together under policy.motion: "reduced" keeps the
// fades (color and opacity carry state; a fade is not movement) but snaps
// everything that travels or scales, "none" snaps it all. Each helper
// returns undefined while its motion is off. A caller's `transition` prop
// overrides a default per property (see withTransitionDefaults in types.ts)
// and `transition={null}` suppresses the defaults outright.

// Scale a pressed control shrinks to; the release springs back.
const PRESS_SCALE = 0.97
// Overshoot of the travel springs (switch knob, segmented indicator): a
// touch of bounce reads as physical without wobble.
const TRAVEL_BOUNCE = 0.15
// Fraction of its size a state mark (checkmark, radio dot) grows in from.
const MARK_FROM_SCALE = 0.4

/** State-change fade for colors and opacity - also the theme cross-fade. */
export function fadeMotion(): Transition | undefined {
  if (policy.motion === "none") return undefined
  return { duration: theme.motion.base, curve: "ease-out" } satisfies Transition
}

/** Press/hover feedback fade (the overlay tints): faster than fadeMotion so a press reads as immediate. */
export function feedbackFade(): Transition | undefined {
  if (policy.motion === "none") return undefined
  return { duration: theme.motion.fast, curve: "ease-out" } satisfies Transition
}

/** Travel spring for a control's moving part. Movement, so "reduced" snaps it too. */
export function travelMotion(bounce: number = TRAVEL_BOUNCE): Transition | undefined {
  if (policy.motion !== "normal") return undefined
  return { duration: theme.motion.slow, bounce } satisfies Transition
}

/** Default `{ color: fade }` declaration for a themed paint node. */
export function colorFade(): { color: Transition } | undefined {
  let fade = fadeMotion()
  return fade === undefined ? undefined : { color: fade }
}

/** Default `{ scale: spring }` declaration for a control that shrinks on press. */
export function scaleFeedback(): { scale: Transition } | undefined {
  if (policy.motion !== "normal") return undefined
  return { scale: { duration: theme.motion.fast } satisfies Transition }
}

/** The scale a pressed control renders at: PRESS_SCALE under full motion, 1 otherwise (a scale snap is worse than none; the overlay tint still marks the press). */
export function pressScale(pressed: boolean): number {
  return pressed && policy.motion === "normal" ? PRESS_SCALE : 1
}

/**
 * Enter/exit fade for an overlay surface mounted in place (modal, bottom
 * sheet): fades in at mount, fades out on removal. The node must set
 * `opacity` explicitly - only carried properties animate.
 */
export function popupFade(): { opacity: Transition } | undefined {
  if (policy.motion === "none") return undefined
  return { opacity: { duration: theme.motion.base, curve: "ease-out", from: 0, exit: 0 } satisfies Transition }
}

/**
 * Exit-only fade for popups that park offscreen until measured (tooltip,
 * menus): they fade in by writing opacity 0 -> 1 once positioned - a
 * mount-time `from` would play unseen - and fade out on removal.
 */
export function popupFadeOut(): { opacity: Transition } | undefined {
  if (policy.motion === "none") return undefined
  return { opacity: { duration: theme.motion.base, curve: "ease-out", exit: 0 } satisfies Transition }
}

/**
 * Enter/exit pop for a control's state mark (checkbox check, radio dot),
 * declared on a wrapper view that sets `opacity={1} scale={1}`: it fades
 * in/out, with a small scale pop under full motion only.
 */
export function markMotion(): { opacity: Transition; scale?: Transition } | undefined {
  if (policy.motion === "none") return undefined
  let declaration: { opacity: Transition; scale?: Transition } = {
    opacity: { duration: theme.motion.base, curve: "ease-out", from: 0, exit: 0 } satisfies Transition,
  }
  if (policy.motion === "normal")
    declaration.scale = {
      duration: theme.motion.slow,
      bounce: TRAVEL_BOUNCE,
      from: MARK_FROM_SCALE,
      exit: MARK_FROM_SCALE,
    } satisfies Transition
  return declaration
}

/**
 * The hover/pressed tint every pressable control draws over its own fill.
 * One always-mounted rect whose hidden state is the same tint at alpha 0,
 * not "transparent": a fade from transparent black would darken a dark
 * scheme's white tint midway. Pressed swaps in the deeper tint; showing,
 * hiding and the pressed/hover swap all fade at feedback speed.
 */
export function PressFeedback(props: {
  pressed?: boolean
  hovered?: boolean
  radius?: number | [number, number, number, number]
}) {
  let tint = () => (props.pressed ? theme.color.overlayPressed : theme.color.overlayHover)
  let fade = () => {
    let f = feedbackFade()
    return f === undefined ? undefined : { color: f }
  }
  return (
    <d-rect
      transition={fade()}
      color={props.pressed || props.hovered ? tint() : withAlpha(tint(), 0)}
      radius={props.radius}
    />
  )
}
