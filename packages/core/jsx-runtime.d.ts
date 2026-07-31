import type {
  WindowProps,
  RectProps,
  OvalProps,
  PathProps,
  ViewProps,
  TextProps,
  TextureProps,
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

  // ref lives on the element prop types (intersected into every entry below)
  // rather than in IntrinsicAttributes: under our config, declaring it only in
  // IntrinsicAttributes did not make it reach intrinsic elements, so `ref` on a
  // host element was reported as an excess property.
  interface ElementRef {
    ref?: Ref<{ id: number }> | undefined
  }

  interface IntrinsicElements {
    window: WindowProps & ElementRef
    view: ViewProps & ElementRef
    text: TextProps & LayoutProps & ElementRef
    rect: RectProps & LayoutProps & ElementRef
    oval: OvalProps & LayoutProps & ElementRef
    path: PathProps & LayoutProps & ElementRef
    texture: TextureProps & LayoutProps & ElementRef
    "d-view": ViewProps & ElementRef
    "d-rect": RectProps & ElementRef
    "d-oval": OvalProps & ElementRef
    "d-path": PathProps & ElementRef
    "d-texture": TextureProps & ElementRef
    "d-text": TextProps & ElementRef
  }
}
