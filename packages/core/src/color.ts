import { colord, extend } from "colord"
import namesPlugin from "colord/plugins/names"
extend([namesPlugin])

/**
 * Parses a CSS color string (named, hex, `rgb()`, `hsl()`, ...) into a packed
 * `0xRRGGBBAA` u32: red in the high byte, alpha in the low byte. Alpha is scaled
 * from colord's 0..1 to 0..255. This is the wire format the runtime expects for
 * the `color` property.
 */
export function parseColor(color: string): number {
  let { r, g, b, a } = colord(color).toRgb()
  return (((r & 0xFF) << 24) | ((g & 0xFF) << 16) | ((b & 0xFF) << 8) | ((a * 255) & 0xFF)) >>> 0
}

// A color stop: `offset` is 0..1 along the gradient, `color` any CSS color string.
export type GradientStop = { offset: number; color: string }

type Stop = { offset: number; color: number }

// A gradient fill value, produced by createLinearGradient / createRadialGradient
// and passed to a paint `color` prop. Coordinates are relative (0..1 of the
// element's box), so one gradient can be reused on elements of any size. Branded
// so the renderer can tell it from a solid color string.
export type Gradient =
  | { readonly __gradient: "linear"; x0: number; y0: number; x1: number; y1: number; stops: Stop[] }
  | { readonly __gradient: "radial"; cx: number; cy: number; r: number; circle: boolean; stops: Stop[] }

/**
 * A linear gradient between two points, each given in 0..1 of the element's box
 * ((0,0) top-left, (1,1) bottom-right). Stops are clamped at the ends.
 */
export function createLinearGradient(
  x0: number, y0: number, x1: number, y1: number, stops: GradientStop[],
): Gradient {
  return { __gradient: "linear", x0, y0, x1, y1, stops: parseStops(stops) }
}

/**
 * A radial gradient centered at `(cx, cy)` (0..1 of the box) with radius `r`
 * (0..1). Defaults to an ellipse that follows the box's aspect ratio; pass
 * `{ shape: "circle" }` to keep a true circle (radius is then a fraction of the
 * shorter side).
 */
export function createRadialGradient(
  cx: number, cy: number, r: number, stops: GradientStop[], opts?: { shape?: "ellipse" | "circle" },
): Gradient {
  return { __gradient: "radial", cx, cy, r, circle: opts?.shape === "circle", stops: parseStops(stops) }
}

export function isGradient(value: unknown): value is Gradient {
  return typeof value === "object" && value !== null && "__gradient" in value
}

// Encodes to the list the runtime decodes (see properties/paint.rs):
//   linear: ["linear", x0, y0, x1, y1, [off0, col0, off1, col1, ...]]
//   radial: ["radial", cx, cy, r, circleFlag, [off0, col0, ...]]
export function encodeGradient(g: Gradient): (string | number | number[])[] {
  let flat: number[] = []
  for (let s of g.stops) flat.push(s.offset, s.color)
  if (g.__gradient === "linear") return ["linear", g.x0, g.y0, g.x1, g.y1, flat]
  return ["radial", g.cx, g.cy, g.r, g.circle ? 1 : 0, flat]
}

function parseStops(stops: GradientStop[]): Stop[] {
  return stops.map((s) => ({ offset: s.offset, color: parseColor(s.color) }))
}