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
  // Per-axis scale; overrides `scale` on that axis (e.g. scaleX for a flip).
  scaleX?: number
  scaleY?: number
  // 3D rotation in radians about the horizontal (rotateX) / vertical (rotateY)
  // axis; reads as real depth only with `perspective` set.
  rotateX?: number
  rotateY?: number
  // Perspective viewing distance in pixels, enabling 3D depth for rotateX/Y.
  perspective?: number
  // Transform origin (the pivot for scale/rotate), in pixels from the box's
  // top-left. Defaults to the box center on each axis.
  originX?: number
  originY?: number
  // Corner radii for the clip applied when overflow is non-visible (hidden,
  // clip, scroll); a single number or [tl, tr, br, bl].
  clipRadius?: number | [number, number, number, number]
  opacity?: number
}

// One choice in an options list, shared by the single-choice controls
// (Select, SegmentedControl). Lives here so those components stay independent
// of each other: shared shapes go through this module, never a sibling import.
export interface Option {
  value: unknown
  label: string
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