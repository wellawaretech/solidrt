import { displayScale } from "@solidrt/core"
import type { LayoutProps, PointerProps, WindowShaderProps } from "@solidrt/core"
import type { StyleProps } from "./types"
import { colorFade } from "./motion"
import { theme } from "./theme"

export interface WindowProps extends PointerProps {
  children?: any
  title?: string
  fullscreen?: boolean
  // The window's own finish, a core window shader declaration forwarded
  // as-is. Unset, the window takes the theme's finish (theme.finish);
  // null opts out of that too.
  shader?: WindowShaderProps | null
  layout?: LayoutProps
  style?: StyleProps
}

// The theme's finish: its declaration with uScale (the display scale, a
// per-client fact the theme cannot know; for effects counted in logical
// px) merged into the params, or nothing while the theme has no program.
function themeFinish(): WindowShaderProps | null {
  let f = theme.finish
  if (f.program == null) return null
  return {
    program: f.program,
    params: { ...f.params, uScale: displayScale() },
    textures: f.textures,
    previous: f.previous,
    vertexCount: f.vertexCount,
  }
}

// A window is the root surface: it can't be transformed or bordered, so only
// the paint-only backgroundColor from style applies here. Its finish is the
// theme's, or the app's own (`shader`).
export function Window(props: WindowProps) {
  return (
    <window
      {...props.layout}
      title={props.title}
      fullscreen={props.fullscreen}
      shader={props.shader === undefined ? themeFinish() : props.shader}
      onPointerEnter={props.onPointerEnter}
      onPointerLeave={props.onPointerLeave}
      onPointerDown={props.onPointerDown}
      onPointerUp={props.onPointerUp}
      onPointerCancel={props.onPointerCancel}
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
        <d-rect transition={colorFade()} color={props.style.backgroundColor} />
      ) : null}
      {props.children}
    </window>
  )
}
