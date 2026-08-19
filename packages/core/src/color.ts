import * as tree from "flux:rendertree"

// Color parsing and mixing live in the runtime (alloy's color module), one
// owner for the CSS grammar and the perceptual math; these re-exports keep
// the core API surface. The renderer no longer parses at all: color strings
// cross the FFI raw and the runtime decodes them.

/**
 * Parses a CSS color string (named, hex, `rgb()`, `hsl()`, ...) into a packed
 * `0xRRGGBBAA` u32: red in the high byte, alpha in the low byte. This is the
 * wire format the runtime expects for the `color` property. Throws on a
 * string that is not a valid CSS color, so a typo fails on the line that
 * wrote it instead of silently painting black.
 */
export function parseColor(color: string): number {
  return tree.parseColor(color)
}

/**
 * Mixes two CSS colors in oklab; `t` is the fraction of `b` (0 = pure `a`,
 * 1 = pure `b`). Returns a hex string. Use it to derive semantic tones
 * (muted text, subtle borders) instead of alpha overlays, so the resulting
 * color does not depend on what is drawn beneath it.
 */
export function mixColors(a: string, b: string, t: number): string {
  return tree.mixColors(a, b, t)
}

/**
 * Perceived brightness of a CSS color, 0 (black) to 1 (white), YIQ-weighted.
 * Compare a text color against its backdrop to decide rendering polarity,
 * e.g. whether a label sits light-on-dark (see typeWeight in components).
 */
export function brightness(color: string): number {
  return tree.brightness(color)
}

// A color stop: `offset` is 0..1 along the gradient, `color` any CSS color string.
export type GradientStop = { offset: number; color: string }

type Stop = { offset: number; color: number }

// A gradient fill value, produced by createLinearGradient / createRadialGradient
// and passed to a paint `color` prop. Coordinates are relative (0..1 of the
// element's box), so one gradient can be reused on elements of any size. Branded
// so the renderer can tell it from a solid color string. The object crosses to
// the runtime as-is and is decoded by key (see properties/paint.rs).
//
// These 0..1 coords are deliberately their own normalized space, NOT the pixel/
// `pct()` length vocabulary used by layout and transformOrigin: a gradient's
// position is naturally a fraction (like a stop offset), so 0..1 reads cleaner
// than pct(0)..pct(100). Do not "unify" them onto pct().
//
// The optional absolute-space fields are produced by parseSvg, never by the
// factories: `units: "absolute"` switches the coordinates to the document's
// drawing space, `spread` is the SVG spreadMethod (default pad), and
// `transform` an SVG matrix(a b c d e f) sextet mapping the gradient's
// coordinates into that space (default identity).
type AbsoluteSpace = {
  units?: "absolute"
  spread?: "pad" | "reflect" | "repeat"
  transform?: [number, number, number, number, number, number]
}

export type Gradient =
  | ({ readonly __gradient: "linear"; x0: number; y0: number; x1: number; y1: number; stops: Stop[] } & AbsoluteSpace)
  | ({ readonly __gradient: "radial"; cx: number; cy: number; r: number; circle?: boolean; stops: Stop[] } & AbsoluteSpace)

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

function parseStops(stops: GradientStop[]): Stop[] {
  return stops.map((s) => ({ offset: s.offset, color: parseColor(s.color) }))
}