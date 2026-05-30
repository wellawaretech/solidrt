import type {
  WindowProps,
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

  // Return type is unconstrained: a ref callback's return value is ignored, so
  // an arrow like `ref={n => (this.node = n)}` (which returns the assignment)
  // is accepted as readily as a void-returning block body.
  type RefCallback<T> = (el: T) => unknown
  type Ref<T> = T | RefCallback<T>

  interface IntrinsicAttributes {
    ref?: Ref<{ id: number }> | undefined
  }

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
