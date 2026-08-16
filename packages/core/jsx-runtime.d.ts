import type {
  WindowProps,
  RectProps,
  OvalProps,
  LineProps,
  PathProps,
  ViewProps,
  ViewOwnProps,
  TextProps,
  SpanProps,
  TextureProps,
  LayoutProps,
  PositionProps,
  GeometryProps,
  OvalGeometryProps,
  TextGeometryProps,
  LineGeometryProps,
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

  // Layout forms compose LayoutProps and derive their geometry from the
  // layout box; d-* forms compose the paint-space geometry props instead.
  interface IntrinsicElements {
    window: WindowProps & ElementRef
    view: ViewProps & ElementRef
    text: TextProps & LayoutProps & ElementRef
    rect: RectProps & LayoutProps & ElementRef
    oval: OvalProps & LayoutProps & ElementRef
    line: LineProps & LayoutProps & ElementRef
    path: PathProps & LayoutProps & ElementRef
    texture: TextureProps & LayoutProps & ElementRef
    "d-view": ViewOwnProps & ElementRef
    "d-rect": RectProps & GeometryProps & ElementRef
    "d-oval": OvalProps & OvalGeometryProps & ElementRef
    "d-line": LineProps & LineGeometryProps & ElementRef
    "d-path": PathProps & PositionProps & ElementRef
    "d-texture": TextureProps & GeometryProps & ElementRef
    "d-text": TextProps & TextGeometryProps & ElementRef
    // A styled run inside <text>/<d-text>; never has a layout box, so there is
    // no d- form.
    span: SpanProps & ElementRef
  }
}
