import { children } from "@solidrt/core"
import type { LayoutProps, PointerProps } from "@solidrt/core"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor, withTransitionDefaults } from "./types"
import { createPress, type PressState } from "./press"
import { colorFade, scaleFeedback } from "./motion"

export type { PressState } from "./press"

export interface PressableProps extends PointerProps, TransitionProps {
  // children and style may be functions of the press state, so a caller can
  // restyle on press/hover without wiring their own signals. The state is live
  // (getters, not a snapshot): read it inside a prop or child expression, never
  // eagerly into a local, or the value is captured once where it was read.
  children?: any | ((state: PressState) => any)
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps | ((state: PressState) => StyleProps)
  // A returned promise sets `state.pending` until it settles; further
  // presses are ignored meanwhile (no double-fire). Non-thenable returns
  // are ignored.
  onPress?: () => unknown
  disabled?: boolean
}

// A pressable box: the createPress semantics (see press.ts) on a styled view.
// When disabled, it takes no pointer events at all. Focus navigation is
// opt-in via `focusable` (Button turns it on by default): a focused Pressable
// activates on Enter/Space/remote-select and exposes `focused` through the
// press state for the caller's ring styling.
export function Pressable(props: PressableProps) {
  let press = createPress(props)

  let style = () => (typeof props.style === "function" ? props.style(press.state()) : props.style)
  // Element-valued props build a fresh native subtree on every read, and a
  // subtree that is never inserted is never destroyed - probing the raw getter
  // with typeof would orphan one full copy per evaluation. children() memoizes
  // the resolve so probe and mount share one build; a render-prop child
  // ((state) => ...) passes through it intact because flatten only unwraps
  // zero-arg functions.
  let resolved = children(() => props.children)
  // The render prop runs once: the state it receives is a live object of getters
  // (see press.ts), so a press or hover updates only the props that read it
  // rather than rebuilding this subtree - which is what keeps a nested
  // recognizer's in-flight gesture alive.
  let kids = () => {
    // children()'s return type erases the render-prop variant, hence the any.
    let c = resolved() as any
    return typeof c === "function" ? c(press.state()) : c
  }

  let hasBackground = () => style()?.backgroundColor != null || style()?.borderRadius != null
  let hasBorder = () => (style()?.borderWidth ?? 0) > 0

  let split = () => splitTransition(props.transition)

  return (
    <view
      // The scale default animates a style-function's press scale, so a
      // custom control built on Pressable presses like Button by default.
      transition={withTransitionDefaults(split().root, scaleFeedback())}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => {
        press.ref(n)
        props.ref?.(n)
      }}
      repaintBoundary
      {...props.layout}
      x={style()?.x}
      y={style()?.y}
      scale={style()?.scale}
      rotate={style()?.rotate}
      opacity={style()?.opacity}
      onPointerEnter={press.handlers.onPointerEnter}
      onPointerLeave={press.handlers.onPointerLeave}
      onPointerDown={press.handlers.onPointerDown}
      onPointerUp={press.handlers.onPointerUp}
      onPointerMove={press.handlers.onPointerMove}
      onWheel={props.onWheel}
      onFocus={press.handlers.onFocus}
      onBlur={press.handlers.onBlur}
      onKeyDown={press.handlers.onKeyDown}
      onKeyUp={props.onKeyUp}
      onTextInput={props.onTextInput}
      focusable={props.focusable === true && props.disabled !== true}
      pointerEvents={props.disabled ? "none" : props.pointerEvents}
    >
      {hasBackground() ? (
        <d-rect
          transition={withTransitionDefaults(split().background, colorFade())}
          onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
          color={style()?.backgroundColor ?? "transparent"}
          radius={style()?.borderRadius}
        />
      ) : null}
      {kids()}
      {hasBorder() ? (
        <d-rect
          drawStyle="stroke"
          transition={withTransitionDefaults(split().border, colorFade())}
          onTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
          color={style()?.borderColor ?? "transparent"}
          strokeWidth={style()?.borderWidth}
          radius={style()?.borderRadius}
        />
      ) : null}
    </view>
  )
}
