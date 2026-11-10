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

  // Declares every name of T as absent. Without it on the laid-out branch, a
  // tag with no discriminant written passes the excess-property check with a
  // name found in any member of the union, so `<rect x={3} />` would typecheck.
  type Never<T> = { [K in keyof T]?: never }

  // The doc comment on `detached` is the hover text, so both members carry it.
  interface LaidOutForm {
    /**
     * Detaches the element from layout: no layout box, the element owns its
     * geometry in paint-space pixels. Set once, as a literal attribute at the
     * tag (`<rect detached>`); it cannot be dynamic and cannot come from a
     * spread. A detached subtree is entirely detached.
     */
    detached?: false
  }
  interface DetachedForm {
    /**
     * Detaches the element from layout: no layout box, the element owns its
     * geometry in paint-space pixels. Set once, as a literal attribute at the
     * tag (`<rect detached>`); it cannot be dynamic and cannot come from a
     * spread. A detached subtree is entirely detached.
     */
    detached: true
  }

  // The detached form of each paired tag, without the discriminant: the
  // deprecated d-* tags take these as they are (a d-* tag is the detached form
  // already, so `detached` on it is rejected).
  interface ViewDetachedProps extends ViewOwnProps, TransitionProps, ElementRef {}
  interface RectDetachedProps extends RectProps, GeometryProps, TransitionProps, ElementRef {}
  interface OvalDetachedProps extends OvalProps, OvalGeometryProps, TransitionProps, ElementRef {}
  interface LineDetachedProps extends LineProps, LineGeometryProps, TransitionProps, ElementRef {}
  interface PathDetachedProps extends PathProps, PositionProps, TransitionProps, ElementRef {}
  interface TextureDetachedProps extends TextureProps, GeometryProps, TransitionProps, ElementRef {}
  interface TextDetachedProps extends TextProps, TextGeometryProps, TransitionProps, ElementRef {}

  // The two branches of each paired tag. Named, so an error prints the name and
  // not the expanded union. The laid-out branch declares the geometry names
  // absent (view needs no guard: ViewOwnProps is a subset of ViewProps).
  interface ViewLaidOut extends ViewProps, TransitionProps, ElementRef, LaidOutForm {}
  interface ViewDetached extends ViewDetachedProps, DetachedForm {}
  interface RectLaidOut extends RectProps, LayoutProps, TransitionProps, ElementRef, LaidOutForm, Never<GeometryProps> {}
  interface RectDetached extends RectDetachedProps, DetachedForm {}
  interface OvalLaidOut extends OvalProps, LayoutProps, TransitionProps, ElementRef, LaidOutForm, Never<OvalGeometryProps> {}
  interface OvalDetached extends OvalDetachedProps, DetachedForm {}
  interface LineLaidOut extends LineProps, LayoutProps, TransitionProps, ElementRef, LaidOutForm, Never<LineGeometryProps> {}
  interface LineDetached extends LineDetachedProps, DetachedForm {}
  interface PathLaidOut extends PathProps, LayoutProps, TransitionProps, ElementRef, LaidOutForm, Never<PositionProps> {}
  interface PathDetached extends PathDetachedProps, DetachedForm {}
  interface TextureLaidOut extends TextureProps, LayoutProps, TransitionProps, ElementRef, LaidOutForm, Never<GeometryProps> {}
  interface TextureDetached extends TextureDetachedProps, DetachedForm {}
  interface TextLaidOut extends TextProps, LayoutProps, TransitionProps, ElementRef, LaidOutForm, Never<TextGeometryProps> {}
  interface TextDetached extends TextDetachedProps, DetachedForm {}

  // Each paired tag is a union discriminated on the literal `detached`: the
  // laid-out branch composes LayoutProps and derives its geometry from the
  // layout box, the detached branch composes the paint-space geometry props.
  // A non-literal `detached={flag}` typechecks (TypeScript splits a boolean
  // across the discriminant); the bundler's compile error rejects it.
  interface IntrinsicElements {
    window: WindowProps & ElementRef
    view: ViewLaidOut | ViewDetached
    text: TextLaidOut | TextDetached
    rect: RectLaidOut | RectDetached
    oval: OvalLaidOut | OvalDetached
    line: LineLaidOut | LineDetached
    path: PathLaidOut | PathDetached
    texture: TextureLaidOut | TextureDetached
    /** @deprecated Write `<view detached>`. */
    "d-view": ViewDetachedProps
    /** @deprecated Write `<rect detached>`. */
    "d-rect": RectDetachedProps
    /** @deprecated Write `<oval detached>`. */
    "d-oval": OvalDetachedProps
    /** @deprecated Write `<line detached>`. */
    "d-line": LineDetachedProps
    /** @deprecated Write `<path detached>`. */
    "d-path": PathDetachedProps
    /** @deprecated Write `<texture detached>`. */
    "d-texture": TextureDetachedProps
    /** @deprecated Write `<text detached>`. */
    "d-text": TextDetachedProps
    // A styled run inside <text>; never has a layout box, so it has no
    // detached form.
    span: SpanProps & ElementRef
  }
}
