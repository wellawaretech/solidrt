import type { LayoutProps, PointerProps } from "@solidrt/core"
import type { StyleProps } from "./types"

export interface WindowProps extends PointerProps {
  children?: any
  title?: string
  fullscreen?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

// A window is the root surface: it can't be transformed or bordered, so only
// the paint-only backgroundColor from style applies here.
export function Window(props: WindowProps) {
  return (
    <window
      {...props.layout}
      title={props.title}
      fullscreen={props.fullscreen}
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
      {props.style?.backgroundColor != null ? (
        <d-rect color={props.style.backgroundColor} />
      ) : null}
      {props.children}
    </window>
  )
}