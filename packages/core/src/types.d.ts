/// <reference types="@solidrt/flux-types" />
/// <reference path="./runtime-modules.d.ts" />

import type { Gradient } from "./color"
import type { ProgramId, TextureBindings, TextureId } from "flux:gpu"
import type { TextInputHints } from "flux:rendertree"
import type { Element } from "solid-js"

export type { TextInputHints }

// The "srt:*" lattice runner modules are declared in ./runtime-modules.d.ts
// (referenced above) - ambient `declare module` only reaches consumers from a
// non-module declaration file, and this file is a module.

declare global {
  interface ImportMeta {
    /**
     * Build-mode constants, substituted textually by the srt bundler (Vite
     * vocabulary). `DEV` is true in dev bundles and false in production
     * bundles, where the substituted constant lets the minifier fold
     * dev-only code away entirely.
     */
    readonly env: { readonly DEV: boolean }
  }

  let speech: {
    start(options: {
      model: Uint8Array, vadModel: Uint8Array, lang?: string, microphone?: number,
      continuous?: boolean, interimResults?: boolean, wakeWord?: Uint8Array | string | string[], wakeThreshold?: number,
    }): Promise<{ handle: number }>
    setResultCallback(handle: number, callback: (result: { transcript: string, isFinal: boolean }) => void): void
    setSpeechStartCallback(handle: number, callback: () => void): void
    setSpeechEndCallback(handle: number, callback: () => void): void
    setWakeCallback(handle: number, callback: () => void): void
    stop(handle: number): void
  }
}

// JSX value model. The element type is solid-js's (its control-flow components
// like <For> return it, so JSX.Element must match). @solidjs/signals ships no
// JSX namespace and solid-js defines no ElementChildrenAttribute, so we supply
// that here - its single key tells TS which prop receives JSX children.
export type { Element }
export interface ElementChildrenAttribute {
  children: {}
}

type Children = Element

// Doc-comment policy for this file: JSX props mostly mirror CSS/DOM, which any
// developer or agent already knows, so a comment restating standard semantics
// is noise. Add one only where a prop deviates from that standard, is bespoke
// to this engine, or has an interaction that isn't decidable from the type
// alone (e.g. shorthand-vs-longhand precedence, a subsetted value range).

interface FlexboxProps {
  gap?: LengthPercentage
  rowGap?: LengthPercentage
  columnGap?: LengthPercentage
  /** Shorthand; overridden by flexGrow/flexShrink/flexBasis when they're also set. */
  flex?: number | "none" | "auto" | (string & {})
  flexGrow?: number
  flexShrink?: number
  flexBasis?: Dimension

  flexDirection?: "row" | "column" | "row-reverse" | "column-reverse"
  flexWrap?: "nowrap" | "wrap" | "wrap-reverse"
  alignSelf?: "start" | "end" | "flex-start" | "flex-end" | "center" | "baseline" | "stretch"
  alignItems?: "start" | "end" | "flex-start" | "flex-end" | "center" | "baseline" | "stretch"
  alignContent?: "start" | "end" | "flex-start" | "flex-end" | "center" | "stretch" | "space-between" | "space-evenly" | "space-around"
  justifyContent?: "start" | "end" | "flex-start" | "flex-end" | "center" | "stretch" | "space-between" | "space-evenly" | "space-around"
}

/** CSS grid subset: line-based placement only, no named lines, no grid-template-areas, and auto tracks take a fixed size (no minmax/fr/keyword). */
interface GridProps {
  gridAutoFlow?: "row" | "column" | "row-dense" | "column-dense"
  gridAutoColumns?: number
  gridAutoRows?: number
  gridColumnStart?: number
  gridColumnEnd?: number
  gridRowStart?: number
  gridRowEnd?: number
  gridTemplateColumns?: string
  gridTemplateRows?: string
}

/**
 * A layout length: a bare number is pixels, `pct(n)` is a percentage of the
 * containing block, plus "auto" and the `"50%"` string form (kept for paste).
 */
type Dimension = number | Pct | "auto" | `${number}%`

/** Like {@link Dimension} without "auto" (e.g. gap, which has no auto value). */
type LengthPercentage = number | Pct | `${number}%`

export interface LayoutProps extends FlexboxProps, GridProps {
  display?: "block" | "flex" | "grid" | "none"
  /**
   * No "fixed" or "sticky". Unlike CSS, `absolute` does not itself become a
   * containing block: an absolute element resolves against the nearest
   * ancestor with `position: relative`, so a chain of absolute elements
   * resolves against whatever relative element is above all of them.
   */
  position?: "relative" | "absolute"

  top?: Dimension
  right?: Dimension
  bottom?: Dimension
  left?: Dimension

  width?: Dimension
  height?: Dimension
  minWidth?: Dimension
  minHeight?: Dimension
  maxWidth?: Dimension
  maxHeight?: Dimension
  aspectRatio?: number | (string & {})

  padding?: Dimension
  paddingTop?: Dimension
  paddingRight?: Dimension
  paddingBottom?: Dimension
  paddingLeft?: Dimension

  margin?: Dimension
  marginTop?: Dimension
  marginRight?: Dimension
  marginBottom?: Dimension
  marginLeft?: Dimension

  overflow?: "visible" | "clip" | "hidden" | "scroll"
  overflowX?: "visible" | "clip" | "hidden" | "scroll"
  overflowY?: "visible" | "clip" | "hidden" | "scroll"

  /**
   * As an inline atom (an element child of <text>): leave the flow and sit
   * against that side of the text, at the top of the line where the atom
   * occurs; the lines it overlaps wrap around its margin box. Same-side
   * floats overlapping vertically sit beside each other. The text's height
   * includes the float. Meaningless outside a <text>.
   */
  float?: "left" | "right"
  /**
   * As an inline atom: start a new line below the text's earlier floats on
   * that side (a floated atom goes below them instead of beside). An empty
   * `<view clear="both" />` is the section break after an image.
   */
  clear?: "left" | "right" | "both"
}

/**
 * A CSS color string: hex (`#rgb`, `#rrggbb`, `#rrggbbaa`), `rgb()`/`rgba()`,
 * `hsl()`/`hsla()`, `hwb()`, or a named color. The CSS Color 4 functions -
 * `color-mix()`, `oklch()`, `lab()`, `color()` - are not parsed and throw
 * `Invalid color`. To mix two colors, `mixColors(a, b, t)` (oklab, returns
 * hex); for a color at a given opacity, `withAlpha(color, a)` (returns
 * `#rrggbbaa`); `parseColor` gives the packed u32 the prop also accepts.
 */
export type Color = string

export type BlendMode = "clear" | "source" | "destination" | "source-over" | "destination-over" | "source-in" | "destination-in" | "source-out" | "destination-out" | "source-atop" | "destination-atop" | "xor" | "plus" | "modulate" | "screen" | "overlay" | "darken" | "lighten" | "color-dodge" | "color-burn" | "hard-light" | "soft-light" | "difference" | "exclusion" | "multiply" | "hue" | "saturation" | "color" | "luminosity"
export type DrawStyle = "fill" | "stroke" | "stroke-and-fill"
export type StrokeCap = "butt" | "round" | "square"
export type StrokeJoin = "miter" | "round" | "bevel"

export interface PaintProps {
  // A solid color, or a gradient from createLinearGradient/createRadialGradient.
  color?: Color | Gradient
  blendMode?: BlendMode
  /** Default "fill"; "stroke" on line, whose segment has no interior (see LineProps). */
  drawStyle?: DrawStyle
  strokeCap?: StrokeCap
  strokeJoin?: StrokeJoin
  strokeMiter?: number
  strokeWidth?: number
}

/**
 * A drop shadow behind a shape (CSS box-shadow field semantics, one shadow):
 * the shape's outer geometry, offset by x/y, grown by `spread`, softened by
 * `blur` (a CSS-style radius in logical px), painted in `color` under the
 * shape. It casts from the shape's outer box whatever the draw style (strokes
 * paint inside, so the box is the outer edge) - except on `path`, where the
 * shadow mirrors the element's own fill/stroke silhouette and `spread` is
 * rejected (an arbitrary path cannot be inflated exactly). `color` is
 * required; there is no currentColor to inherit.
 */
export interface ShadowProps {
  x?: number
  y?: number
  blur?: number
  spread?: number
  color: Color
}

/**
 * A subtree filter (the CSS filter functions, object form): the view's
 * children are composited together, run through the set color operations -
 * fused into one color matrix, applied in the fixed order grayscale, sepia,
 * saturate, hueRotate, brightness, contrast, invert - and then blurred.
 * `blur` is a CSS-style radius in logical px; `hueRotate` is radians (this
 * API's angle convention); the rest are the CSS amounts (1 = unchanged for
 * saturate/brightness/contrast, 0 = unchanged for the others; grayscale,
 * sepia and invert saturate at 1). There is no `opacity` key - the view's
 * own `opacity` prop is the same compositing layer. Like opacity, a filter
 * on a non-boundary view costs a save_layer; on a repaintBoundary view it
 * rides the composite for free. Hit testing is unaffected.
 */
export interface FilterProps {
  blur?: number
  grayscale?: number
  sepia?: number
  saturate?: number
  hueRotate?: number
  brightness?: number
  contrast?: number
  invert?: number
}

/** A percentage value, from `pct(50)`. Resolves against the element box. */
export type Pct = { readonly __unit: "pct"; v: number }

// One axis of the transform origin (the point rotate/scale/3D pivot around),
// split per axis to match the engine's x/y prop convention. A bare number is
// pixels; `pct(50)` is a fraction of the box, so a percentage origin tracks the
// layout size with no reactive wiring. Unset defaults to the axis center on a
// laid-out view; on a d-view (no box of its own) it defaults to the view's
// local (0,0) - the origin its children's coordinates are authored against, so
// the pivot never depends on the inherited box. To pivot a d-view around its
// content's center, set the origin explicitly in pixels; pct()/keywords on a
// d-view resolve against the inherited box, which is rarely what you want.
type OriginX = number | Pct | "left" | "center" | "right"
type OriginY = number | Pct | "top" | "center" | "bottom"

export interface TransformProps {
  rotate?: number
  scale?: number
  // Per-axis scale; overrides `scale` on that axis (e.g. scaleX for a flip).
  scaleX?: number
  scaleY?: number
  // 3D rotation about the horizontal axis, in radians (a top/bottom tilt). Like
  // rotateY, reads as 3D only with `perspective` set.
  rotateX?: number
  // 3D rotation about the vertical axis, in radians, for a card-flip. Reads as a
  // real flip only with `perspective` set; on its own it is an orthographic
  // squash (like scaleX).
  rotateY?: number
  // Perspective viewing distance in pixels (CSS `perspective`). Enables the 3D
  // depth for rotateY; larger values give a shallower effect.
  perspective?: number
  // Subtree translation in pixels, composited post-layout (no re-record, no
  // layout). Unlike the draw primitives' detached-only x/y, these exist on
  // layout views too - the drag/thumb idiom animates them freely.
  x?: number
  y?: number
  originX?: OriginX
  originY?: OriginY
  // Group opacity in 0..1: children are composited together, then faded as a
  // whole (CSS `opacity`). Does not affect hit testing. Costs a compositing
  // layer (save_layer around the subtree) while below 1, except on a
  // repaintBoundary view, where it is hoisted to composite time for free. To
  // fade a single primitive, put the alpha in its `color` (rgba) instead -
  // paint alpha costs nothing.
  opacity?: number
  scrollX?: number
  scrollY?: number
}

// Window-relative pointer coordinates are reported as clientX/clientY (matching
// the DOM MouseEvent). pointerType distinguishes mouse from touch; button is the
// pressed button on down/up (0 = primary); the modifier flags mirror the DOM.
export interface PointerEvent {
  clientX: number
  clientY: number
  /**
   * Pointer position in the coordinate frame of the node whose handler is
   * running (its transform chain undone), so it differs per node as the event
   * bubbles. Exact even when the pointer is not over the node: a drag routed
   * along the frozen down-path keeps reporting true local coordinates after
   * leaving it.
   */
  localX: number
  localY: number
  /**
   * Pointer position in the frame the running node's own x/y coordinates live
   * in: its parent on the hit path (the window for the root). The drag idiom
   * is `x = parentX - grab offset`, with the grab offset taken from
   * localX/localY at pointer down. The path parent skips
   * pointerEvents="none" ancestors, so it is the layout parent in ordinary
   * trees.
   */
  parentX: number
  parentY: number
  /**
   * Pointer movement since the previous move event, in logical pixels. Mouse
   * reports hardware deltas (summed, never lost, and the only motion signal
   * while the pointer is locked); touch reports position diffs. 0 on
   * non-move events.
   */
  movementX: number
  movementY: number
  /** Node id whose handler is currently running (bubbling changes it per call). */
  currentTarget: number
  /** Deepest node id of the event's path (the hit leaf). */
  target: number
  pointerId: number
  pointerType: "mouse" | "touch" | "pen" | (string & {})
  button?: number
  shiftKey: boolean
  ctrlKey: boolean
  altKey: boolean
  metaKey: boolean
  // Stops the event from reaching ancestor handlers. Events dispatch leaf->root
  // (bubbling), so calling this in a child prevents the enclosing node from
  // seeing it (e.g. a slider claiming a drag so an ancestor scroller ignores it).
  stopPropagation: () => void
}

export interface WheelEvent extends PointerEvent {
  deltaX: number
  deltaY: number
}

// Key events use the W3C UI Events vocabulary: `key` is the logical,
// layout-dependent value ("a", "!", "Enter", "ArrowLeft"); `code` is the
// physical, layout-independent key position ("KeyA", "Digit1", "NumpadEnter").
// Printable characters for text entry arrive via onTextInput, not here.
// Routing: keydown/keyup dispatch along the focused node's ancestor chain,
// leaf->root, always ending at the window root; with nothing focused they go
// to the window root alone. <window onKeyDown> is therefore the app-global
// shortcut point.
export interface KeyEvent {
  key: string
  code: string
  repeat: boolean
  shiftKey: boolean
  ctrlKey: boolean
  altKey: boolean
  metaKey: boolean
  /** Node id whose handler is currently running (bubbling changes it per call). */
  currentTarget: number
  /** Node id the dispatch started at: the focused node, or the window root when nothing is focused. */
  target: number
  /**
   * Stops the event from reaching ancestor handlers. A component that consumed
   * the key calls this so enclosing handlers (and app-global shortcuts on the
   * window) do not also act on it.
   */
  stopPropagation: () => void
}

export interface TextEvent {
  text: string
}

export interface PointerProps {
  onPointerDown?: (event: PointerEvent) => void
  onPointerUp?: (event: PointerEvent) => void
  onPointerMove?: (event: PointerEvent) => void
  onPointerEnter?: (event: PointerEvent) => void
  onPointerLeave?: (event: PointerEvent) => void
  onWheel?: (event: WheelEvent) => void
  onFocus?: () => void
  onBlur?: () => void
  onKeyDown?: (event: KeyEvent) => void
  onKeyUp?: (event: KeyEvent) => void
  onTextInput?: (event: TextEvent) => void
  /**
   * IME behavior for this node's text-entry sessions (keyboard type,
   * capitalization, autocorrect); read when a session starts. Without it the
   * OS defaults apply - notably sentence auto-capitalization, which
   * identifier fields and terminals want off:
   * `textInputHints={{ capitalize: "none", autocorrect: false }}`.
   */
  textInputHints?: TextInputHints
  /**
   * Declares the element a candidate for focus navigation, enumerable via
   * getFocusables(). Candidacy only - it changes no behavior by itself; focus
   * still moves through setFocus.
   */
  focusable?: boolean
  pointerEvents?: "auto" | "none" | "all"
}

/**
 * Detached-only geometry, in paint-space pixels. Never affects layout: these
 * props exist only on the d-* forms, where there is no layout box and the
 * element owns its geometry. The layout forms of the draw primitives derive
 * their geometry from the layout box instead (size it with width/height).
 */
export interface PositionProps {
  /** Horizontal offset of the drawn geometry; defaults to 0. */
  x?: number
  /** Vertical offset of the drawn geometry; defaults to 0. */
  y?: number
}

/** See {@link PositionProps}: detached-only, never affects layout. */
export interface GeometryProps extends PositionProps {
  /** Drawn width; defaults to the inherited box width. */
  w?: number
  /** Drawn height; defaults to the inherited box height. */
  h?: number
}

/** See {@link PositionProps}: detached-only, never affects layout. */
export interface OvalGeometryProps extends PositionProps {
  /** Bounding box width of the ellipse (not a radius); defaults to the inherited box. */
  w?: number
  /** Bounding box height of the ellipse (not a radius); defaults to the inherited box. */
  h?: number
}

/**
 * See {@link PositionProps}: detached-only, never affects layout. A d-text
 * is boxed or anchored. Boxed (no `anchor`): it wraps at `w`, or at the box
 * inherited from the nearest laid-out ancestor, and `textAlign` aligns
 * lines inside that width - so a right-aligned label needs a `w`. Anchored:
 * `x` is a point on the line (SVG's text-anchor), which is what a label on a
 * chart, an axis or a diagram wants. Either way the reported bounds
 * (getBoundingBox, the tree) are the laid-out paragraph - widest line by
 * line stack - unless `w`/`h` override a side.
 */
export interface TextGeometryProps extends PositionProps {
  /**
   * Where `x` sits on each line: its start, middle or end. With `w` unset
   * the text shapes at its natural width (no wrap; `\n` still breaks), and
   * `textAlign` defaults to the anchor's side. With `w` set, the `w`-wide
   * box is what gets anchored at `x`.
   */
  anchor?: "start" | "middle" | "end"
  // Shaping (wrap) width; unset, boxed text wraps at the inherited box and
  // anchored text does not wrap.
  w?: number
  // Reported-bounds height override only; paragraph height always falls out
  // of the text.
  h?: number
}

/**
 * See {@link PositionProps}: detached-only, never affects layout. The
 * endpoints (or `points`) are the line's own geometry; `x`/`y` offset all of
 * it, the way a `d-path`'s offset its `d`, so one write moves a polyline. A
 * line's reported bounds (getBoundingBox, the tree, a detached capture) are
 * its painted box: the geometry's extent plus the stroke's reach, not the
 * inherited box.
 */
export interface LineGeometryProps extends PositionProps {
  /** Endpoints default to spanning the box: (0,0) to (box width, box height). */
  x1?: number
  y1?: number
  x2?: number
  y2?: number
}

// Native transitions (okf/done/native-transitions.md): declared once on
// the element, applied by the runtime to every later write of the covered
// properties. JS hands over targets; Rust interpolates every frame, so a
// running animation costs no JS per frame.

/** A cubic-bezier timing curve: a CSS name or [x1, y1, x2, y2] control values. */
export type TransitionCurve = "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | [number, number, number, number]

/**
 * A perceptual spring - the default kind: a bare `{ duration }` is a
 * critically damped spring. `duration` (ms) is the perceptual settling
 * time, `bounce` in (-1, 1] the springiness - 0 (the default) settles
 * without overshoot, positive values overshoot, negative values settle
 * sluggishly. A new target while the spring runs keeps position and
 * velocity, so the motion stays continuous - use springs for anything
 * retargeted while moving.
 */
export interface TransitionSpring {
  duration: number
  bounce?: number
  /** Hold each write for this long (ms) before it applies; a newer write during the hold replaces it and restarts the delay. */
  delay?: number
  /**
   * Mount-time enter animation: at the element's first attach the property
   * snaps to this value and animates to the value it mounted with. Numbers
   * for the scalar properties; the color property takes a CSS color string
   * or packed number. Per-property entries only (not under `all`); a later
   * move or reorder re-runs nothing.
   */
  from?: number | string
  /**
   * Removal exit animation: an unmounted element stays visible, animates
   * the property to this value (honoring `delay`), and is freed when its
   * exit animations settle. Same value forms as `from`, per-property only.
   * A move never plays it, the exiting element is hit-test invisible, its
   * whole subtree stays painted with it, and no onTransitionEnd fires (the
   * component is already disposed). An attached element keeps its layout
   * slot until the exit finishes.
   */
  exit?: number | string
}

/**
 * A duration/curve tween, opted into by naming the curve (a tween is
 * always a specific curve; without one the spec reads as a spring).
 * Duration in ms. A new target while the tween runs restarts it from the
 * current value with the full duration (CSS semantics) - designer-timed,
 * one-shot motion.
 */
export interface TransitionTween {
  duration: number
  curve: TransitionCurve
  /** Hold each write for this long (ms) before it applies; a newer write during the hold replaces it and restarts the delay. */
  delay?: number
  /**
   * Mount-time enter animation: at the element's first attach the property
   * snaps to this value and animates to the value it mounted with. Numbers
   * for the scalar properties; the color property takes a CSS color string
   * or packed number. Per-property entries only (not under `all`); a later
   * move or reorder re-runs nothing.
   */
  from?: number | string
  /**
   * Removal exit animation: an unmounted element stays visible, animates
   * the property to this value (honoring `delay`), and is freed when its
   * exit animations settle. Same value forms as `from`, per-property only.
   * A move never plays it, the exiting element is hit-test invisible, its
   * whole subtree stays painted with it, and no onTransitionEnd fires (the
   * component is already disposed). An attached element keeps its layout
   * slot until the exit finishes.
   */
  exit?: number | string
}

/**
 * The shorthand string: `"<duration>ms [curve] [<delay>ms]"` - `"300ms"` is
 * a bounce-0 spring, `"300ms ease-out"` a tween, `"300ms ease-out 100ms"`
 * delayed (first time value the duration, second the delay; ms only).
 * Bounce, bezier control values and `from` need the object form.
 */
export type TransitionShorthand = string

export type Transition = TransitionSpring | TransitionTween | TransitionShorthand

/** The property names a transition can cover (numeric scalars). */
export type TransitionPropName =
  | "x"
  | "y"
  | "w"
  | "h"
  | "x1"
  | "y1"
  | "x2"
  | "y2"
  | "scrollX"
  | "scrollY"
  | "opacity"
  | "originX"
  | "originY"
  | "perspective"
  | "clipRadius"
  | "srcX"
  | "srcY"
  | "srcW"
  | "srcH"
  | "onLength"
  | "offLength"
  | "dashOffset"
  | "rotate"
  | "rotateX"
  | "rotateY"
  | "scale"
  | "scaleX"
  | "scaleY"
  | "strokeWidth"
  | "radius"
  | "color"

/** Payload of onTransitionEnd: which animated property finished. */
export interface TransitionEndEvent {
  property: TransitionPropName
}

export interface TransitionProps {
  /**
   * A runtime-side transition of one of this element's properties reached
   * its target (natural settles only; a cancelled or retargeted animation
   * does not fire until it finally settles). Delivered to this element
   * only, no bubbling.
   */
  onTransitionEnd?: (event: TransitionEndEvent) => void
  /**
   * Animate later writes of the listed properties instead of snapping:
   * `transition={{ x: { duration: 400, bounce: 0.2 }, opacity: "200ms ease-out" }}`.
   * `all` covers every animatable property the element has, and a bare
   * string is shorthand for it: `transition="300ms ease-out"`. Only
   * properties the element carries animate (a d-rect has x, a view's x is
   * its transform); the initial value never animates unless the entry sets
   * `from` (an enter animation), and a non-numeric write (e.g. null)
   * cancels the running animation and snaps. `null` clears the
   * declaration; already-running animations finish. A spec built in a
   * conditional widens `curve` to `string` for TypeScript; write
   * `satisfies Transition` on the branch.
   */
  transition?:
    | ({
        all?: Omit<TransitionSpring, "from" | "exit"> | Omit<TransitionTween, "from" | "exit"> | TransitionShorthand
        /**
         * Group stagger (ms): every descendant enter (`from`) or exit that
         * begins in the same frame under this element gets `index * stagger`
         * of extra delay, in occurrence order (enters and exits cascade
         * separately). Nearest declaring ancestor wins; it orchestrates
         * descendants only - ordinary writes and this element's own
         * lifecycle are unaffected. Adds on top of a per-entry `delay`.
         */
        stagger?: number
      } & {
        [P in TransitionPropName]?: Transition
      })
    | TransitionShorthand
    | null
}

// Primitives

export interface WindowProps extends LayoutProps, PointerProps, TransitionProps {
  children?: Children
  title?: string
  fullscreen?: boolean
  /**
   * Run the window's finished frame through a GPU program as the last step
   * before it reaches the screen. While declared, the frame renders into a
   * runtime-owned layer texture the program samples; removing the prop
   * restores the direct path and frees the layer. Everything else about the
   * program (compiling, linking, lifetime) is the raw shading layer's:
   * see compileShader/linkProgram.
   */
  shader?: WindowShaderProps | null
}

/**
 * A window shader declaration. The program reads the frame through
 * `uniform sampler2D uSource` (top-left origin, like every sampled texture -
 * so a vertex stage mapping it onto the window flips the v coordinate) and
 * is drawn attributeless as triangles, `vertexCount` vertices fetched via
 * gl_VertexID. `iResolution`, the window size in physical pixels (the pass
 * covers exactly that), is filled by name into a uniform the program declares
 * itself or gets from compileShader's `header` option. The window is cleared
 * to opaque black first, so geometry that does not cover it still presents a
 * defined frame.
 */
export interface WindowShaderProps {
  /** Linked program handle from linkProgram. */
  program: ProgramId
  /**
   * Uniforms filled by name, paced to the next real repaint. A number drives
   * a scalar (`float`/`int`); a flat number array (or a Float32Array /
   * Float64Array) drives the declared GLSL type: 2/3/4 for
   * `vec2`/`vec3`/`vec4`, 16 (column-major) for `mat4`. An undefined or null
   * entry is skipped.
   */
  params?: Record<string, number | number[] | Float32Array | Float64Array>
  /** Extra sampler2D inputs: uniform name to texture id, or `{ id, filter?, wrap? }` for a per-binding sampling override. An undefined or null entry is skipped. */
  textures?: TextureBindings
  /** Vertices drawn (attributeless triangles). Default 3, the covering triangle. */
  vertexCount?: number
  /**
   * Retain the last frame as a second layer the program samples as
   * `uniform sampler2D uPrevious` (one-frame history: motion echo, frame
   * differencing). Costs one extra window-sized texture while declared.
   * Until a second frame exists uPrevious samples opaque black. Only declare
   * the uPrevious uniform together with this flag - without it the uniform
   * stays at unit 0 and aliases uSource. Default false.
   */
  previous?: boolean
}

// Everything a view offers besides layout: d-view uses this directly (a
// detached view has no taffy presence, so layout props would be rejected at
// runtime); the layout `view` adds LayoutProps below.
export interface ViewOwnProps extends TransformProps, PointerProps {
  children?: Children
  trace?: boolean
  /**
   * Design-space size `[w, h]` (both positive) for the children: everything
   * under the view - layout, paint, input - happens in that coordinate space,
   * which is uniformly scaled to fit and centered in the element's box (SVG's
   * viewBox with its default preserveAspectRatio, generalized off the graphics
   * format and onto a layout element; the fit is always "contain"). Laid-out
   * children resolve flex, percentages and text wrapping against the design
   * size, so a subtree scales into any box without reflowing. The view itself
   * sizes like a replaced element: its intrinsic size is the design size, one
   * sized axis derives the other from the design aspect, layout props
   * override, and it always shrinks to fit (its min-content size is zero). As
   * a flex item it still stretches like any other under the default alignment:
   * a width-only design-size view in a row takes the line's height, not the
   * design height, unless the row's alignItems or its own alignSelf is not
   * "stretch" (CSS's rule for an <img> in a flex row). Composed innermost: the
   * transform props still operate in box space, and pointer events on children
   * arrive in design coordinates. The overflow clip and scrollX/scrollY stay
   * box properties: the clip rect is the layout box and scroll offsets are box
   * pixels, regardless of fit scale. The natural wrapper for parseSvg draws,
   * any d-* subtree authored in fixed design units, or a whole panel that
   * should scale rather than reflow.
   */
  designSize?: [number, number]
  /**
   * Corner radii for the clip applied when overflow is non-visible (hidden,
   * clip, scroll on both axes). A single number rounds all four corners; an
   * array is [top-left, top-right, bottom-right, bottom-left]. Without overflow
   * clipping this has no effect.
   */
  clipRadius?: number | [number, number, number, number]
  /**
   * Marks a repaint boundary: the subtree is recorded into its own retained
   * display list and reused until something inside it changes. Place around
   * heavy static content that sits next to frequently changing content.
   *
   * "snapshot" additionally retains the rasterized pixels as a GPU texture,
   * skipping rasterization entirely. Costs texture memory and re-rasterizes
   * on layout-size or display-scale changes. Content painted outside the
   * element's layout box is cropped, and ancestor scale animations smear the
   * bitmap; best for screen-aligned, static, raster-expensive content. On a
   * d-view the box is the inherited frame (the nearest laid-out ancestor's
   * box, or the design size under a designSize view), same as a capture.
   *
   * "snapshot-no-aa" is "snapshot" rasterized without anti-aliasing: cheaper
   * (no multisampled scratch, one render pass), but vector content - svg
   * paths, rounded corners, rotated edges - comes out hard-edged. Text and
   * axis-aligned rects look identical, so prefer it for plain UI panels.
   *
   * A snapshot boundary's pixels are available to the GPU stack as a live
   * texture id through `snapshotTexture(ref)`.
   */
  repaintBoundary?: boolean | "snapshot" | "snapshot-no-aa"
  /**
   * Run this view's rasterized subtree through a GPU program and composite
   * the result in its place. Requires a snapshot boundary
   * (repaintBoundary="snapshot" or "snapshot-no-aa"; the cost is snapshot
   * semantics, kept explicit; declared without one the shader is ignored
   * with a warning). The pass is region-sized and split from content
   * invalidation: a params-only change re-runs just the pass against the
   * cached snapshot, so animating an effect over a static subtree never
   * re-rasterizes it.
   *
   * The effect samples only the subtree's own pixels - grading, warping or
   * dissolving the panel works; anything needing what is behind it does not.
   * Sampling outside the content clamps to the edge, and the output is
   * cropped to the layout box like any snapshot. Hit-testing stays on layout
   * geometry: a distortion moves pixels, not hit targets. The view's own
   * transform and opacity apply after the effect, so the program sees
   * unrotated, opaque content.
   */
  shader?: ViewShaderProps | null
  filter?: FilterProps
  /**
   * Filters the pixels already painted BENEATH the view's box, in place,
   * before the view's own content draws - frosted glass over live content
   * (CSS `backdrop-filter`). Same keys and semantics as `filter`. The
   * region is the layout box; give the view an overflow clip with
   * `clipRadius` for rounded glass. Costs an offscreen capture of the
   * pixels beneath at that point in the frame, so treat it as a deliberate
   * panel, not a casual style. Inside a `repaintBoundary="snapshot"`
   * subtree it sees only that boundary's own offscreen content, never what
   * is behind the boundary (the same containment `blendMode` has there).
   */
  backdropFilter?: FilterProps
}

/**
 * A boundary shader declaration. The program contract matches shader targets,
 * not the window pass: the subtree's rasterization binds as
 * `uniform sampler2D uSource` (top-left origin, like every sampled texture)
 * and the pass draws the covering triangle attributeless. `iResolution`, the
 * boundary in physical pixels, is filled by name into a uniform the program
 * declares itself or gets from compileShader's `header` option.
 */
export interface ViewShaderProps {
  /** Linked program handle from linkProgram. */
  program: ProgramId
  /**
   * Uniforms filled by name, paced to the next real repaint. A number drives
   * a scalar (`float`/`int`); a flat number array (or a Float32Array /
   * Float64Array) drives the declared GLSL type: 2/3/4 for
   * `vec2`/`vec3`/`vec4`, 16 (column-major) for `mat4`. An undefined or null
   * entry is skipped.
   */
  params?: Record<string, number | number[] | Float32Array | Float64Array>
  /** Extra sampler2D inputs: uniform name to texture id, or `{ id, filter?, wrap? }` for a per-binding sampling override. An undefined or null entry is skipped. */
  textures?: TextureBindings
  /**
   * Transparent margin in logical px on every side of the layout box, for
   * the effect to write into - glow, drop shadow, blur that bleeds past the
   * edge. Grows the rasterized canvas and the composited quad; the subtree's
   * own paint stays clipped to the layout box either way. The pass sees only
   * the bigger iResolution - declare an app uniform if the program needs the
   * margin size. Default 0.
   */
  outset?: number
  /**
   * Retain the prior rasterization of the subtree as
   * `uniform sampler2D uPrevious`. Source history, not output history: it
   * rotates when the content actually re-rasterizes, not per frame - on a
   * content change uPrevious holds exactly the old look (transition
   * material: cross-dissolve old into new), while for a static subtree with
   * animated params uPrevious equals uSource. Feedback/accumulation is not
   * this; that stays with manual targets. Costs one extra canvas-sized
   * texture while declared; transparent until the first rotation, and reset
   * to transparent by a size or scale change. Only declare the uPrevious
   * uniform together with this flag - without it the uniform stays at unit 0
   * and aliases uSource. Default false.
   */
  previous?: boolean
}

export interface ViewProps extends ViewOwnProps, LayoutProps {}

// draw primitives

// A stroked rect paints inside its box, like a CSS border: the stroke's outer
// edge sits on the box edge rather than straddling it, so nothing bleeds past
// the box for a clip to cut. `path` and `line` strokes stay centered on their
// geometry - there the geometry is the stroke, not a box. A dashed stroke
// dashes that same inset outline (see DashProps).
export interface RectProps extends PaintProps, PointerProps, DashProps {
  // Corner radius, measured on the box (the stroke's outer edge). A single
  // number applies to all four corners; an array is [top-left, top-right,
  // bottom-right, bottom-left] (CSS border-radius order).
  radius?: number | [number, number, number, number]
  shadow?: ShadowProps
}

// Strokes paint inside the box, same as `RectProps`.
export interface OvalProps extends PaintProps, PointerProps, DashProps {
  shadow?: ShadowProps
}

/**
 * A stroke's dash pattern, on every stroked primitive (`rect`, `oval`,
 * `line`, `path`). Both lengths must be set to dash; with either unset, or
 * a gap of 0, the stroke is solid. A box primitive dashes its inset
 * outline, the pattern starting where SVG's does (a rect on the top edge
 * after the top-left corner, an oval at 3 o'clock) and running clockwise;
 * its dashes stay inside the box like the solid stroke.
 */
export interface DashProps {
  /**
   * The drawn length, in local units. The pattern runs continuously along
   * the geometry - through a polyline's vertices and along a path's curves,
   * restarting at each subpath of a path; 0 draws a dot per period (given
   * round or square caps).
   */
  onLength?: number
  /** The gap length, in local units. */
  offLength?: number
  /**
   * Distance into the dash pattern at which the stroke starts, in local
   * units (SVG stroke-dashoffset). Wraps around the pattern's period;
   * negative values allowed. Raising it marches the dashes toward the
   * geometry's start: write it every frame for marching ants, or transition
   * it for a one-shot slide. Default 0.
   */
  dashOffset?: number
  /**
   * What the geometry's length counts as, in the pattern's units (SVG
   * pathLength): when set, `onLength`, `offLength` and `dashOffset` are
   * scaled by the actual length over it. `pathLength={1}` makes them
   * fractions: `onLength={0.77} offLength={1}` draws the first 77%, and
   * transitioning `onLength` from 0 to 1 draws the geometry on. Must be
   * positive; unset, the pattern is in local units.
   */
  pathLength?: number
}

// A line's geometry is numbers, not a path string: the primitive to reach
// for when the geometry moves (each endpoint is one property write, a
// polyline is one array write; a path animates by rebuilding its `d` string).
// Two forms: the segment, whose endpoints (x1/y1/x2/y2) exist on the
// detached `d-line` only, and the polyline (`points`), which exists on both.
// A laid-out `<line>` without points is practically a rule - give it a thin
// box (length x strokeWidth); in general it draws its layout box's
// top-left-to-bottom-right diagonal. For arbitrary angles and connectors use
// `d-line`; for curves, a path. A line's stroke is centered on its geometry,
// so it straddles the box (a rect's paints inside), and the bounds it
// reports are the painted box: geometry plus stroke, on both forms.
//
// The paint defaults to `drawStyle="stroke"` (the box primitives default to
// fill). On a polyline "fill" and "stroke-and-fill" fill the polygon
// (nonzero, implicitly closed) and hit-test its interior; on the two-point
// form fill has no effect, a segment has no interior.
export interface LineProps extends PaintProps, PointerProps, DashProps {
  /**
   * Polyline vertices as a flat [x0, y0, x1, y1, ...] in the element's local
   * space (the space x1..y2 use). Takes precedence over the endpoints while
   * set. Content, not box geometry: a laid-out <line points> measures its
   * box from the points' extent, like a <path> from `d`, and draws them
   * unscaled. Fewer than two points draws nothing; an odd count throws. Not
   * covered by transitions: animate by writing a new array.
   */
  points?: number[] | Float32Array | Float64Array
  /**
   * Close the polyline's stroke: the segment back to the first point, joined
   * there instead of capped. A fill always covers the polygon (closed
   * implicitly), so this is a stroke distinction. Default false.
   */
  closed?: boolean
}

/**
 * `d` is an SVG path string; the stroke is centered on the geometry. The
 * bounds a path reports (getBoundingBox, the tree, a detached capture) are
 * its painted box: the geometry's tight extent (curve extrema, not control
 * points) plus the stroke's reach, at a `d-path`'s x/y - not its layout box
 * or the inherited one.
 */
export interface PathProps extends PaintProps, PointerProps, DashProps {
  d?: string
  fillRule?: "nonzero" | "evenodd"
  /** Shadows the drawn silhouette (fill and/or stroke); `spread` is rejected here. */
  shadow?: ShadowProps
}

export type FontFamily = "sans" | "serif" | "mono" | (string & {})
export type FontStyle = "normal" | "italic"
export type FontWeight = 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
export type TextDecoration = "none" | "underline"
export type TextAlign = "left" | "right" | "center" | "justify"
export type TextOverflow = "clip" | "ellipsis" | (string & {})

/**
 * Per-run text style: the paragraph default on <text>, an override on <span>.
 * The cascade is intra-paragraph only: a span inherits from its enclosing
 * span, then from the <text>; nothing inherits across the tree.
 */
export interface TextRunProps {
  fontFamily?: FontFamily
  fontSize?: number
  /**
   * Line height as a MULTIPLIER of fontSize, not pixels (the theme uses
   * 1.3-1.6). A CSS-reflex pixel value like 22 makes each line box 22x the
   * font size, rendering the text as blank space.
   */
  lineHeight?: number
  fontStyle?: FontStyle
  fontWeight?: FontWeight
  /**
   * Underline in the run's own color, drawn straight through descenders
   * (no skip-ink). Position and thickness come from the font's own metrics
   * unless overridden; a font Impeller resolves through the system fallback
   * gets the shipped Noto values.
   */
  textDecoration?: TextDecoration
  /** Pixels from the baseline to the top of the underline. */
  textUnderlineOffset?: number
  /** Underline thickness in pixels. */
  textDecorationThickness?: number
}

/**
 * A styled run inside <text>. Inline text only: children are text and other
 * spans. `color` takes what a text's color takes (solid or gradient).
 * Pointer handlers fire for the boxes of the run's own text on each line it
 * spans and bubble to the enclosing spans and text.
 */
export interface SpanProps extends TextRunProps, PointerProps {
  children?: Children
  color?: Color | Gradient
}

export interface TextProps extends PaintProps, PointerProps, TextRunProps {
  children?: Children
  textAlign?: TextAlign
  maxLines?: number
  /**
   * What happens to text cut off by maxLines: "clip" (default), "ellipsis"
   * (a U+2026 at the end of the last line), or any other string to use as
   * the ellipsis. Drawn in the paragraph's default style.
   */
  textOverflow?: TextOverflow
  /**
   * A word (wrap unit) wider than the line: "anywhere" (default) splits it
   * at grapheme boundaries so it stays inside the box, "normal" keeps it
   * whole and lets it overflow (CSS's default).
   */
  overflowWrap?: "normal" | "anywhere"
  /**
   * First-line indent in pixels. Negative hangs: the first line starts at 0
   * and every following line is indented by the magnitude. A hard break does
   * not start a new first line.
   */
  textIndent?: number
  /**
   * How lines are chosen beyond greedy fitting (CSS text-wrap): "wrap"
   * (default) is greedy; "balance" evens the line lengths while keeping the
   * line count (headings, captions); "pretty" is greedy except that a lone
   * word on the last line pulls one down from the line above. Neither
   * applies once maxLines truncates.
   */
  textWrap?: "wrap" | "balance" | "pretty"
}

/**
 * A raster draw uses only part of a paint. `blendMode` applies, which is how
 * two GPU layers composite in the tree (a solid pass plus an additive pass)
 * without a hand-written compositing shader. `color` contributes its alpha
 * only, as an opacity multiplier; its RGB does not tint, and a gradient does
 * not replace the texture. `drawStyle` and the stroke props have no effect.
 * Texture alpha is premultiplied, so additive modes need no manual
 * premultiplication.
 */
export interface TextureProps extends PaintProps, PointerProps {
  src?: TextureId
  /**
   * How the texture's pixels map to the element box (CSS object-fit).
   * "fill" (default) stretches; "cover" and "none" crop; "contain" and
   * "scale-down" letterbox. Everything centers - there is no object-position.
   * Paint-only: the element box itself is unaffected, so "contain" letterbox
   * bars and "cover" cropped edges still hit-test as part of the element.
   */
  fit?: "fill" | "cover" | "contain" | "none" | "scale-down"
  srcX?: number
  srcY?: number
  srcW?: number
  srcH?: number
  // Shader uniform values, when src names a render target: the same channel
  // setTargetParams drives, written through it directly - prop and
  // imperative writes validate and error identically (an unknown name
  // throws, and set src before params: a write with no src to route to
  // throws). However often a signal writes, the target renders once per
  // frame at the raster flush. A number drives a scalar (`float`/`int`); a
  // flat number array (or a Float32Array / Float64Array) drives the declared
  // GLSL type: 2/3/4 for `vec2`/`vec3`/`vec4`, 16 (column-major) for `mat4`.
  // An undefined or null entry is skipped.
  params?: Record<string, number | number[] | Float32Array | Float64Array>
}
