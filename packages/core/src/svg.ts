// SVG documents as data: parseSvg turns a document string into a flat list of
// draws that map straight onto <d-path> elements. Vector currency is path
// data, the same way raster currency is a texture id - there is no document
// element that swallows the source; the app owns the parsed data and composes
// ordinary primitives from it.

import { parseSvg as fluxParseSvg } from "flux:svg"
import { parseColor, type Gradient } from "./color"
import type { StrokeCap, StrokeJoin } from "./types"

/**
 * Tags an inline SVG source, returning it unchanged. Documents small enough to
 * belong beside the code that uses them stay in the file; the tag is what
 * makes them legible there, because editors highlight markup inside a template
 * literal only when a known tag marks it (the name matters - `svg` is one the
 * grammars look for). Raw semantics, like `glsl`.
 */
export let svg = String.raw

/**
 * One resolved draw in document coordinates. The keys match `PathProps`, so a
 * draw spreads onto a `<d-path>` unchanged: `<d-path {...draw} />`. A source
 * path with both a fill and a stroke yields two draws (fill first).
 */
export type SvgDraw = {
  d: string
  color: string | Gradient
  drawStyle: "fill" | "stroke"
  fillRule?: "nonzero" | "evenodd"
  strokeWidth?: number
  strokeCap?: StrokeCap
  strokeJoin?: StrokeJoin
}

/** A parsed document: intrinsic size (viewBox/width-height) plus the flat draw list. */
export type SvgDocument = {
  width: number
  height: number
  draws: SvgDraw[]
}

/**
 * Parses an SVG document string (an imported `.svg` asset, an icon library's
 * string export, or a template literal) into plain draw data: geometry
 * flattened to absolute path data with every transform baked in, paints
 * resolved to colors or gradients. Render it by wrapping the draws in a view
 * that fits the document's coordinate space into its box:
 *
 *     let doc = createMemo(() => parseSvg(src))
 *     <view repaintBoundary designSize={[doc().width, doc().height]} width={48} height={48}>
 *       {doc().draws.map((draw) => <d-path {...draw} />)}
 *     </view>
 *
 * The plain repaintBoundary is the recommended default: the parsed subtree is
 * static, so it never re-records alongside changing siblings. Each `<d-path>`
 * hit-tests its exact outline; when the document should act as ONE hit target
 * (the usual icon case), add `pointerEvents="all"` to the wrapper - the box
 * then matches as a whole and the per-path outline tests are skipped.
 *
 * `opts.color` drives `currentColor` in the document (any CSS color string),
 * which is how monochrome icon sets (Lucide, Feather, Heroicons, ...) get
 * recolored; explicit fills/strokes still win. Parsing is synchronous and
 * sandboxed (no network, file, or data-URI access) and throws on an invalid
 * document. Parse once per document under a memo, not per instance.
 *
 * Unsupported and skipped: clipPath, masks, filters, patterns, embedded
 * images, and SVG text.
 */
export function parseSvg(src: string, opts?: { color?: string }): SvgDocument {
  if (opts?.color != null) return fluxParseSvg(src, { color: parseColor(opts.color) })
  return fluxParseSvg(src)
}
