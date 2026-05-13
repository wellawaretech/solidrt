import type {
  WindowProps,
  CircleProps,
  RectProps,
  OvalProps,
  PathProps,
  ViewProps,
  TextProps,
  TextureProps,
  AudioProps,
  LayoutProps
} from "./src/types"
import type { JSX as SolidJSX } from "@solidjs/signals"

export namespace JSX {
  type Element = SolidJSX.Element
  type ElementChildrenAttribute = SolidJSX.ElementChildrenAttribute

  interface IntrinsicElements {
    window: WindowProps
    view: ViewProps
    text: TextProps & LayoutProps
    rect: RectProps & LayoutProps
    oval: OvalProps & LayoutProps
    path: PathProps & LayoutProps
    texture: TextureProps & LayoutProps
    audio: AudioProps
    "d-rect": RectProps
    "d-oval": OvalProps
    "d-path": PathProps
    "d-texture": TextureProps
    "d-text": TextProps
  }
}
