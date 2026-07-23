import { children } from "@solidrt/core"
import type { LayoutProps, PointerProps } from "@solidrt/core"
import type { StyleProps } from "./types"
import { createPress, type PressState } from "./press"

export type { PressState } from "./press"

export interface PressableProps extends PointerProps {
  // children and style may be functions of the press state, so a caller can
  // restyle on press/hover without wiring their own signals.
  children?: any | ((state: PressState) => any)
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps | ((state: PressState) => StyleProps)
  onPress?: () => void
  disabled?: boolean
}

// A pressable box: the createPress semantics (see press.ts) on a styled view.
// When disabled, it takes no pointer events at all.
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
  let kids = () => {
    let c = resolved()
    return typeof c === "function" ? c(press.state()) : c
  }

  let hasBackground = () => style()?.backgroundColor != null || style()?.borderRadius != null
  let hasBorder = () => (style()?.borderWidth ?? 0) > 0

  return (
    <view
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
      onFocus={props.onFocus}
      onBlur={props.onBlur}
      onKeyDown={props.onKeyDown}
      onKeyUp={props.onKeyUp}
      onTextInput={props.onTextInput}
      pointerEvents={props.disabled ? "none" : props.pointerEvents}
    >
      {hasBackground() ? (
        <d-rect
          color={style()?.backgroundColor ?? "transparent"}
          radius={style()?.borderRadius}
        />
      ) : null}
      {kids()}
      {hasBorder() ? (
        <d-rect
          drawStyle="stroke"
          color={style()?.borderColor ?? "transparent"}
          strokeWidth={style()?.borderWidth}
          radius={style()?.borderRadius}
        />
      ) : null}
    </view>
  )
}
