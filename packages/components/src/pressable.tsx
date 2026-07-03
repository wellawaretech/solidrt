import { createSignal } from "@solidjs/signals"
import type { LayoutProps, PointerEvent, PointerProps } from "@solidrt/core"
import type { StyleProps } from "./types"

export type PressState = { pressed: boolean; hovered: boolean }

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

// A pressable box. onPress fires on a primary-button down followed by an up over
// the same node. Because there is no pointer capture, a drag that leaves the box
// fires onPointerLeave, which cancels the press (no onPress) and clears hover --
// this also covers the no-up-outside case. Non-primary buttons (right/middle) do
// not start a press. When disabled, it takes no pointer events at all.
export function Pressable(props: PressableProps) {
  let [pressed, setPressed] = createSignal(false)
  let [hovered, setHovered] = createSignal(false)

  let state = (): PressState => ({ pressed: pressed(), hovered: hovered() })
  let style = () => (typeof props.style === "function" ? props.style(state()) : props.style)
  let kids = () => (typeof props.children === "function" ? props.children(state()) : props.children)

  let handleDown = (e: PointerEvent) => {
    if (e.button != null && e.button !== 0) return
    setPressed(true)
    props.onPointerDown?.(e)
  }
  let handleUp = (e: PointerEvent) => {
    if (pressed()) props.onPress?.()
    setPressed(false)
    props.onPointerUp?.(e)
  }
  let handleEnter = (e: PointerEvent) => {
    setHovered(true)
    props.onPointerEnter?.(e)
  }
  let handleLeave = (e: PointerEvent) => {
    setHovered(false)
    setPressed(false)
    props.onPointerLeave?.(e)
  }

  let hasBackground = () => style()?.backgroundColor != null || style()?.borderRadius != null
  let hasBorder = () => (style()?.borderWidth ?? 0) > 0

  return (
    <view
      ref={props.ref}
      repaintBoundary
      {...props.layout}
      x={style()?.x}
      y={style()?.y}
      scale={style()?.scale}
      rotate={style()?.rotate}
      onPointerEnter={handleEnter}
      onPointerLeave={handleLeave}
      onPointerDown={handleDown}
      onPointerUp={handleUp}
      onPointerMove={props.onPointerMove}
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