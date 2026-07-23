import { createSignal } from "@solidrt/core"
import type { PointerEvent } from "@solidrt/core"

export type PressState = { pressed: boolean; hovered: boolean }

export interface PressOptions {
  onPress?: () => void
  onPointerDown?: (e: PointerEvent) => void
  onPointerUp?: (e: PointerEvent) => void
  onPointerEnter?: (e: PointerEvent) => void
  onPointerLeave?: (e: PointerEvent) => void
}

// The press state machine shared by the pressable components. onPress fires on
// a primary-button down followed by an up over the same node. There is no
// pointer capture, so a drag that leaves the node fires onPointerLeave, which
// cancels the press (no onPress) and clears hover -- this also covers the
// no-up-outside case. Non-primary buttons (right/middle) do not start a press
// and are not forwarded. Options are read at event time, so passing a
// component's reactive props object keeps handler changes live. Deliberately
// framework-agnostic (no theme, no styling): a candidate for promotion into
// core once the recognizer family grows (okf/plans/component-gestures.md).
export function createPress(options: PressOptions) {
  let [pressed, setPressed] = createSignal(false)
  let [hovered, setHovered] = createSignal(false)

  let state = (): PressState => ({ pressed: pressed(), hovered: hovered() })

  let handlers = {
    onPointerDown: (e: PointerEvent) => {
      if (e.button != null && e.button !== 0) return
      setPressed(true)
      options.onPointerDown?.(e)
    },
    onPointerUp: (e: PointerEvent) => {
      if (pressed()) options.onPress?.()
      setPressed(false)
      options.onPointerUp?.(e)
    },
    onPointerEnter: (e: PointerEvent) => {
      setHovered(true)
      options.onPointerEnter?.(e)
    },
    onPointerLeave: (e: PointerEvent) => {
      setHovered(false)
      setPressed(false)
      options.onPointerLeave?.(e)
    },
  }

  return { pressed, hovered, state, handlers }
}
