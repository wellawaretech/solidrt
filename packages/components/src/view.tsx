import type { LayoutProps, PointerProps } from "@solidrt/core"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"

export interface ViewProps extends PointerProps, TransitionProps {
  children?: any
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
}

export function View(props: ViewProps) {
  let hasBackground = () =>
    props.style?.backgroundColor != null || props.style?.borderRadius != null
  let hasBorder = () => (props.style?.borderWidth ?? 0) > 0

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={props.ref}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      scaleX={props.style?.scaleX}
      scaleY={props.style?.scaleY}
      rotate={props.style?.rotate}
      rotateX={props.style?.rotateX}
      rotateY={props.style?.rotateY}
      perspective={props.style?.perspective}
      originX={props.style?.originX}
      originY={props.style?.originY}
      clipRadius={props.style?.clipRadius}
      opacity={props.style?.opacity}
      onPointerEnter={props.onPointerEnter}
      onPointerLeave={props.onPointerLeave}
      onPointerDown={props.onPointerDown}
      onPointerUp={props.onPointerUp}
      onPointerMove={props.onPointerMove}
      onWheel={props.onWheel}
      onFocus={props.onFocus}
      onBlur={props.onBlur}
      onKeyDown={props.onKeyDown}
      onKeyUp={props.onKeyUp}
      onTextInput={props.onTextInput}
      pointerEvents={props.pointerEvents}
    >
      {hasBackground() ? (
        <d-rect
          transition={split().background}
          onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
          color={props.style?.backgroundColor ?? "transparent"}
          radius={props.style?.borderRadius}
        />
      ) : null}
      {props.children}
      {hasBorder() ? (
        <d-rect
          drawStyle="stroke"
          transition={split().border}
          onTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
          color={props.style?.borderColor ?? "transparent"}
          strokeWidth={props.style?.borderWidth}
          radius={props.style?.borderRadius}
        />
      ) : null}
    </view>
  )
}