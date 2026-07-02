/// <reference types="@solidrt/flux-types" />
/// <reference path="./runtime-modules.d.ts" />

import type { Gradient } from "./color"
import type { Element } from "solid-js"

// The "srt:*" lattice runner modules are declared in ./runtime-modules.d.ts
// (referenced above) - ambient `declare module` only reaches consumers from a
// non-module declaration file, and this file is a module.

declare global {
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

interface FlexboxProps {
  gap?: number
  rowGap?: number
  columnGap?: number
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

type Dimension = number | "auto" | `${number}%`

export interface LayoutProps extends FlexboxProps, GridProps {
  display?: "block" | "flex" | "grid" | "none"
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
  cx?: number
  cy?: number
  scrollX?: number
  scrollY?: number
}

// Window-relative pointer coordinates are reported as clientX/clientY (matching
// the DOM MouseEvent). pointerType distinguishes mouse from touch; button is the
// pressed button on down/up (0 = primary); the modifier flags mirror the DOM.
export interface PointerEvent {
  clientX: number
  clientY: number
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

export interface KeyEvent {
  key: string
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

export interface WindowProps extends LayoutProps {
  children?: Children
  title?: string
  fullscreen?: boolean
  vsync?: boolean
  fps?: boolean
}

export interface ViewProps extends LayoutProps, TransformProps, PointerProps {
  children?: Children
  trace?: boolean
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
   */
  repaintBoundary?: boolean | "snapshot"
}

export interface AudioProps {
  src?: Uint8Array
  play?: number
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
  w?: number
  h?: number
}

export interface LineProps extends PaintProps, PointerProps {
  x1?: number
  y1?: number
  x2?: number
  y2?: number
  onLength?: number
  offLength?: number
}

export interface PathProps extends Position, PaintProps, PointerProps {
  d?: string
  fillRule?: "nonzero" | "evenodd"
}

export interface SvgProps extends Position, PointerProps {
  // A whole SVG document as a string (an imported asset, a fetched string, or a
  // template literal). Parsed and rendered as one unit; takes no JSX children.
  src?: string
  // Drives currentColor in the document. Explicit fills/strokes still win.
  color?: Color
}

export interface TextProps extends Position, PaintProps, PointerProps {
  children?: Children
  // Shaping (wrap) width. Detached text wraps at the inherited ancestor size
  // by default; set w for an unwrapped natural line or an explicit wrap width.
  w?: number
  // Reported-bounds height only; paragraph height always falls out of the text.
  h?: number
  fontFamily?: "sans" | "mono" | (string & {})
  fontSize?: number
  lineHeight?: number
  fontStyle?: "normal" | "italic"
  fontWeight?: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
  textAlign?: "left" | "right" | "center" | "justify"
  maxLines?: number
}

export interface TextureProps extends Position {
  src?: number
  imageWidth?: number
  imageHeight?: number
  srcX?: number
  srcY?: number
  srcW?: number
  srcH?: number
  params?: Record<string, number>
}
