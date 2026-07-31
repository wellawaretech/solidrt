/// <reference types="@solidrt/flux-types" />
/// <reference path="./runtime-modules.d.ts" />

import type { Gradient } from "./color"
import type { ProgramId, TextureId } from "flux:gpu"
import type { Element } from "solid-js"

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

  let image: {
    decodeImage(bytes: Uint8Array): { data: Uint8Array, width: number, height: number }
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
}

/** Colors are CSS color strings, parsed to a packed u32 by `parseColor`. */
export type Color = string

export interface PaintProps {
  // A solid color, or a gradient from createLinearGradient/createRadialGradient.
  color?: Color | Gradient
  blendMode?: "clear" | "source" | "destination" | "source-over" | "destination-over" | "source-in" | "destination-in" | "source-out" | "destination-out" | "source-atop" | "destination-atop" | "xor" | "plus" | "modulate" | "screen" | "overlay" | "darken" | "lighten" | "color-dodge" | "color-burn" | "hard-light" | "soft-light" | "difference" | "exclusion" | "multiply" | "hue" | "saturation" | "color" | "luminosity"
  drawStyle?: "fill" | "stroke" | "stroke-and-fill"
  strokeCap?: "butt" | "round" | "square"
  strokeJoin?: "miter" | "round" | "bevel"
  strokeMiter?: number
  strokeWidth?: number
}

/** A percentage value, from `pct(50)`. Resolves against the element box. */
export type Pct = { readonly __unit: "pct"; v: number }

// One axis of the transform origin (the point rotate/scale/3D pivot around),
// split per axis to match the engine's x/y prop convention. A bare number is
// pixels; `pct(50)` is a fraction of the box, so a percentage origin tracks the
// layout size with no reactive wiring. Unset defaults to the axis center.
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
  x?: number
  y?: number
  originX?: OriginX
  originY?: OriginY
  // Group opacity in 0..1: children are composited together, then faded as a
  // whole (CSS `opacity`). Does not affect hit testing.
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
export interface KeyEvent {
  key: string
  code: string
  repeat: boolean
  shiftKey: boolean
  ctrlKey: boolean
  altKey: boolean
  metaKey: boolean
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
  pointerEvents?: "auto" | "none" | "all"
}

interface Position {
  x?: number
  y?: number
}

// Primitives

export interface WindowProps extends LayoutProps, PointerProps {
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
 * gl_VertexID. `iResolution`, filled by name, is the window size in physical
 * pixels (the pass covers exactly that). The window is cleared to opaque
 * black first, so geometry that does not cover it still presents a defined
 * frame.
 */
export interface WindowShaderProps {
  /** Linked program handle from linkProgram. */
  program: ProgramId
  /**
   * Uniforms filled by name, paced to the next real repaint. A number drives
   * a scalar (`float`/`int`); a flat number array drives the declared GLSL
   * type: 2/3/4 for `vec2`/`vec3`/`vec4`, 16 (column-major) for `mat4`.
   */
  params?: Record<string, number | number[]>
  /** Extra sampler2D inputs: uniform name to texture id. */
  textures?: Record<string, TextureId>
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

export interface ViewProps extends LayoutProps, TransformProps, PointerProps {
  children?: Children
  trace?: boolean
  /**
   * Design-space size `[w, h]` for the children: content drawn in that
   * coordinate space is uniformly scaled to fit and centered in the element's
   * box (SVG's default preserveAspectRatio, generalized). A pure fit
   * transform - it never sizes the element, so give the box its size with
   * layout props. Composed innermost: the transform props still operate in
   * box space, and pointer events on children arrive in design coordinates.
   * The natural wrapper for parseSvg draws, or any d-* subtree authored in
   * fixed design units.
   */
  viewBox?: [number, number]
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
   * bitmap; best for screen-aligned, static, raster-expensive content.
   *
   * "snapshot-no-aa" is "snapshot" rasterized without anti-aliasing: cheaper
   * (no multisampled scratch, one render pass), but vector content - svg
   * paths, rounded corners, rotated edges - comes out hard-edged. Text and
   * axis-aligned rects look identical, so prefer it for plain UI panels.
   */
  repaintBoundary?: boolean | "snapshot" | "snapshot-no-aa"
}

// draw primitives

export interface RectProps extends Position, PaintProps, PointerProps {
  w?: number
  h?: number
  // Corner radius. A single number applies to all four corners; an array is
  // [top-left, top-right, bottom-right, bottom-left] (CSS border-radius order).
  radius?: number | [number, number, number, number]
}

export interface OvalProps extends Position, PaintProps, PointerProps {
  /** Bounding box width of the ellipse (not a radius); defaults to the layout box. */
  w?: number
  /** Bounding box height of the ellipse (not a radius); defaults to the layout box. */
  h?: number
}

export interface LineProps extends PaintProps, PointerProps {
  x1?: number
  y1?: number
  x2?: number
  y2?: number
  /** Dash pattern in local units: the drawn segment length. Both onLength and offLength must be set to dash; with either unset the line is solid. */
  onLength?: number
  /** Dash pattern in local units: the gap length. Both onLength and offLength must be set to dash; with either unset the line is solid. */
  offLength?: number
}

export interface PathProps extends Position, PaintProps, PointerProps {
  d?: string
  fillRule?: "nonzero" | "evenodd"
}

export interface TextProps extends Position, PaintProps, PointerProps {
  children?: Children
  // Shaping (wrap) width. Detached text wraps at the inherited ancestor size
  // by default; set w for an unwrapped natural line or an explicit wrap width.
  w?: number
  // Reported-bounds height only; paragraph height always falls out of the text.
  h?: number
  fontFamily?: "sans" | "serif" | "mono" | (string & {})
  fontSize?: number
  /**
   * Line height as a MULTIPLIER of fontSize, not pixels (the theme uses
   * 1.3-1.6). A CSS-reflex pixel value like 22 makes each line box 22x the
   * font size, rendering the text as blank space.
   */
  lineHeight?: number
  fontStyle?: "normal" | "italic"
  fontWeight?: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
  textAlign?: "left" | "right" | "center" | "justify"
  maxLines?: number
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
export interface TextureProps extends Position, PaintProps, PointerProps {
  src?: TextureId
  /**
   * How the texture's pixels map to the element box (CSS object-fit).
   * "fill" (default) stretches; "cover" and "none" crop; "contain" and
   * "scale-down" letterbox. Everything centers - there is no object-position.
   * Paint-only: the element box itself is unaffected, so "contain" letterbox
   * bars and "cover" cropped edges still hit-test as part of the element.
   */
  fit?: "fill" | "cover" | "contain" | "none" | "scale-down"
  w?: number
  h?: number
  srcX?: number
  srcY?: number
  srcW?: number
  srcH?: number
  // Shader uniform values, when src names a shader texture. Applied at the
  // next repaint (not synchronously), so a fast-changing signal stays paced
  // to real frames rather than triggering a GL render pass per write. A
  // number drives a scalar (`float`/`int`); a flat number array drives the
  // declared GLSL type: 2/3/4 for `vec2`/`vec3`/`vec4`, 16 (column-major)
  // for `mat4`.
  params?: Record<string, number | number[]>
}
