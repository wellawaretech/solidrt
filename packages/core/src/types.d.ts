import type { Accessor, JSX as SolidJSX } from "@solidjs/signals"

declare global {
  let Flux: {
    on(event: string, callback: (data: any) => void): () => void
  }

  let ffi: {
    createRoot(id: number): void
    createNode(id: number, kind: string): void
    insertNode(parentId: number, nodeId: number, anchorId?: number): void
    deleteNode(parentId: number, nodeId: number): void
    setProperty(nodeId: number, name: string, value: unknown): void
  }
}

type Children = SolidJSX.Element

type OA<T> = T | Accessor<T>

interface FlexboxProps {
  gap?: number
  rowGap?: number
  columnGap?: number
  flexGrow?: number
  flexShrink?: number
  flexBasis?: number

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
}

import type { LCH } from "./color"
export type Color = string | LCH

export interface PaintProps {
  color?: Color
  blendMode?: "clear" | "source" | "destination" | "source-over" | "destination-over" | "source-in" | "destination-in" | "source-out" | "destination-out" | "source-atop" | "destination-atop" | "xor" | "plus" | "modulate" | "screen" | "overlay" | "darken" | "lighten" | "color-dodge" | "color-burn" | "hard-light" | "soft-light" | "difference" | "exclusion" | "multiply" | "hue" | "saturation" | "color" | "luminosity"
  drawStyle?: "fill" | "stroke" | "stroke-and-fill"
  strokeCap?: "butt" | "round" | "square"
  strokeJoin?: "miter" | "round" | "bevel"
  strokeMiter?: number
  strokeWidth?: number
}

export interface TransformProps {
  rotate?: OA<number>
  scale?: OA<number>
  x?: OA<number>
  y?: OA<number>
}

export interface PointerProps {
  onPointerDown?: Function
  onPointerUp?: Function
  onPointerMove?: Function
  onPointerEnter?: Function
  onPointerLeave?: Function
  onWheel?: Function
  onFocus?: Function
  onBlur?: Function
  onKeyDown?: Function
  onKeyUp?: Function
  onTextInput?: Function
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
  vsync?: boolean
  fps?: boolean
}

export interface ViewProps extends LayoutProps, TransformProps, PointerProps {
  children?: Children
  trace?: boolean
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

export interface PathProps extends Position, PaintProps, PointerProps {
  d?: string
  fillRule?: "nonZero" | "evenOdd"
}

export interface TextProps extends PaintProps {
  children?: Children
  fontSize?: number
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
