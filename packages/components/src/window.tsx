import type { LayoutProps } from "@solidrt/core"
import type { StyleProps } from "./types"

export interface WindowProps {
  children?: any
  title?: string
  fullscreen?: boolean
  vsync?: boolean
  fps?: boolean
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
      vsync={props.vsync}
      fps={props.fps}
    >
      {props.style?.backgroundColor != null ? (
        <d-rect color={props.style.backgroundColor} />
      ) : null}
      {props.children}
    </window>
  )
}