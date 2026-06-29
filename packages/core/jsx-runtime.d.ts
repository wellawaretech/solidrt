import type {
  WindowProps,
  RectProps,
  OvalProps,
  PathProps,
  SvgProps,
  ViewProps,
  TextProps,
  TextureProps,
  AudioProps,
  LayoutProps,
  Element as CoreElement,
  ElementChildrenAttribute as CoreElementChildrenAttribute
} from "./src/types"

export namespace JSX {
  type Element = CoreElement
  type ElementChildrenAttribute = CoreElementChildrenAttribute

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
    svg: SvgProps & LayoutProps
    texture: TextureProps & LayoutProps
    audio: AudioProps
    "d-view": ViewProps
    "d-rect": RectProps
    "d-oval": OvalProps
    "d-path": PathProps
    "d-svg": SvgProps
    "d-texture": TextureProps
    "d-text": TextProps
  }
}
