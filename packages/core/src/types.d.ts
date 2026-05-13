import type { Accessor, JSX as SolidJSX } from "@solidjs/signals"
import { mappings } from "./constants"

type Children = SolidJSX.Element

type OA<T> = T | Accessor<T>

interface FlexboxProps {
  gap?: number
  rowGap?: number
  columnGap?: number
  flexGrow?: number
  flexShrink?: number
  flexBasis?: number

  flexDirection?: keyof typeof mappings.flexDirection
  flexWrap?: keyof typeof mappings.flexWrap
  alignSelf?: keyof typeof mappings.alignSelf
  alignItems?: keyof typeof mappings.alignItems
  alignContent?: keyof typeof mappings.alignContent
  justifyContent?: keyof typeof mappings.justifyContent
}

interface GridProps {
  gridAutoFlow?: keyof typeof mappings.gridAutoFlow
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
  display?: keyof typeof mappings.display
  position?: keyof typeof mappings.position

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

  overflow?: keyof typeof mappings.overflow
}

import type { LCH } from "./color"
export type Color = string | LCH

export interface PaintProps {
  color?: Color
  blendMode?: keyof typeof mappings.blendMode
  drawStyle?: keyof typeof mappings.drawStyle
  strokeCap?: keyof typeof mappings.strokeCap
  strokeJoin?: keyof typeof mappings.strokeJoin
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
  pointerEvents?: keyof typeof mappings.pointerEvents
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
  r?: number
}

export interface OvalProps extends Position, PaintProps, PointerProps {
  w?: number
  h?: number
}

export interface PathProps extends Position, PaintProps, PointerProps {
  d?: string
  fillRule?: keyof typeof mappings.fillRule
}

export interface TextProps extends PaintProps {
  children: Children
  fontSize?: number
  fontStyle?: keyof typeof mappings.fontStyle
  textAlign?: keyof typeof mappings.textAlign
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
