import type { Color, Gradient, LayoutProps } from "@solidrt/core"

// The split between layout and style follows one rule: layout properties feed
// into Taffy and changing them triggers a relayout; style properties are
// paint-only and can change without affecting layout.

// Paint-only props. None of these change the box Taffy computes. Borders are
// drawn as a stroke overlay (not part of the box model), and the transform is
// applied at paint time, so both live here.
export interface StyleProps {
  color?: Color | Gradient
  backgroundColor?: Color | Gradient
  borderColor?: Color | Gradient
  borderWidth?: number
  borderRadius?: number | [number, number, number, number]
  x?: number
  y?: number
  rotate?: number
  scale?: number
  opacity?: number
}

// Text shaping affects measurement, so font props belong with layout rather
// than style. These end up on the inner <text> node, while the box layout
// fields go on the wrapping <view>.
export interface TextLayoutProps extends LayoutProps {
  fontFamily?: "sans" | "mono" | (string & {})
  fontSize?: number
  lineHeight?: number
  fontStyle?: "normal" | "italic"
  fontWeight?: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
  textAlign?: "left" | "right" | "center" | "justify"
  maxLines?: number
}