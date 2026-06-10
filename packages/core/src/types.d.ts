/// <reference types="@solidrt/flux-types" />

import type { JSX as SolidJSX } from "@solidjs/signals"

// UI event bus (lattice), provided by the runtime as a builtin module.
// on/once return an unsubscribe function.
declare module "srt:events" {
  export function on(event: string, callback: (data: any) => void): () => void
  export function once(event: string, callback: (data: any) => void): () => void
}

// Dev-server control surface (lattice). Present only in dev/go builds; in other
// builds `available` is false and the functions are no-ops.
declare module "srt:dev" {
  export const available: boolean
  export const canDiscover: boolean
  export const recents: string[]
  export function connect(address: string): void
  export function discover(): void
  export function stop(): void
}

declare global {
  function requestAnimationFrame(callback: (time: number) => void): number
  function cancelAnimationFrame(id: number): void

  let ffi: {
    createRoot(id: number): void
    createNode(id: number, kind: string): void
    insertNode(parentId: number, nodeId: number, anchorId?: number): void
    deleteNode(parentId: number, nodeId: number): void
    setProperty(nodeId: number, name: string, value: unknown): void
    setTextInputActive(active: boolean): void
    measureText(text: string, options?: MeasureTextOptions): { width: number, height: number }
    getBoundingBox(id: number): { x: number, y: number, width: number, height: number } | null
  }

  let gpu: {
    createTexture(data: Uint8Array, width: number, height: number): number
    createMutableTexture(data: Uint8Array, width: number, height: number): number
    uploadTexture(textureId: number, offset?: number): void
    decodeImage(bytes: Uint8Array): { data: Uint8Array, width: number, height: number }
  }

  let camera: {
    listCameras(): { id: number, name: string, facing: "front" | "back" | "unknown" }[]
    open(options: { camera?: number, facing?: "front" | "back", width?: number, height?: number, scan?: string[] }):
      Promise<{ handle: number, texture: number, width: number, height: number }>
    setBarcodeCallback(handle: number, callback: (result: { data: string, format: "qr" }) => void): void
    scanImage(data: Uint8Array, width: number, height: number): { data: string, format: "qr" }[]
    close(handle: number): void
  }

  let microphone: {
    listMicrophones(): { id: number, name: string }[]
    open(options: { microphone?: number, sampleRate?: number }): { handle: number, sampleRate: number }
    read(handle: number): Float32Array
    close(handle: number): void
  }
}

export interface MeasureTextOptions {
  fontFamily?: "sans" | "mono" | (string & {})
  fontSize?: number
  fontStyle?: "normal" | "italic"
  fontWeight?: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
  maxLines?: number
}

type Children = SolidJSX.Element

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

// Colors are CSS color strings, parsed to a packed u32 by parseColorToU32.
export type Color = string

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
  rotate?: number
  scale?: number
  x?: number
  y?: number
  cx?: number
  cy?: number
  scrollX?: number
  scrollY?: number
}

export interface PointerEvent {
  x: number
  y: number
}

export interface WheelEvent {
  x: number
  y: number
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
  fillRule?: "nonZero" | "evenOdd"
}

export interface TextProps extends PaintProps, PointerProps {
  children?: Children
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
