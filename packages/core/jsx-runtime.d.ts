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
  TransitionProps,
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
    view: ViewProps & TransitionProps & ElementRef
    text: TextProps & LayoutProps & TransitionProps & ElementRef
    rect: RectProps & LayoutProps & TransitionProps & ElementRef
    oval: OvalProps & LayoutProps & TransitionProps & ElementRef
    line: LineProps & LayoutProps & TransitionProps & ElementRef
    path: PathProps & LayoutProps & TransitionProps & ElementRef
    texture: TextureProps & LayoutProps & TransitionProps & ElementRef
    "d-view": ViewOwnProps & TransitionProps & ElementRef
    "d-rect": RectProps & GeometryProps & TransitionProps & ElementRef
    "d-oval": OvalProps & OvalGeometryProps & TransitionProps & ElementRef
    "d-line": LineProps & LineGeometryProps & TransitionProps & ElementRef
    "d-path": PathProps & PositionProps & TransitionProps & ElementRef
    "d-texture": TextureProps & GeometryProps & TransitionProps & ElementRef
    "d-text": TextProps & TextGeometryProps & TransitionProps & ElementRef
    // A styled run inside <text>/<d-text>; never has a layout box, so there is
    // no d- form.
    span: SpanProps & ElementRef
  }
}
