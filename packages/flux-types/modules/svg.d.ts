declare module "flux:svg" {
  /**
   * A gradient paint from a parsed document, in the branded shape the render
   * tree decodes: coordinates are absolute document-space values
   * (`units: "absolute"`), `spread` is the SVG spreadMethod (absent = pad),
   * and `transform` an SVG matrix(a b c d e f) sextet mapping the gradient's
   * coordinates into the document space (absent = identity). Stop colors are
   * packed `0xRRGGBBAA` numbers.
   */
  export type SvgGradient =
    | {
        __gradient: "linear"
        units: "absolute"
        x0: number
        y0: number
        x1: number
        y1: number
        stops: { offset: number; color: number }[]
        spread?: "reflect" | "repeat"
        transform?: [number, number, number, number, number, number]
      }
    | {
        __gradient: "radial"
        units: "absolute"
        cx: number
        cy: number
        r: number
        stops: { offset: number; color: number }[]
        spread?: "reflect" | "repeat"
        transform?: [number, number, number, number, number, number]
      }

  /**
   * One resolved draw in document coordinates. The keys deliberately match
   * the path element's props, so a draw spreads onto a `<d-path>` unchanged.
   * A source path with both a fill and a stroke yields two draws (fill
   * first). Solid colors are `#rrggbbaa` strings; stroke keys are only
   * present on stroke draws, `fillRule` only on fills.
   */
  export type SvgDraw = {
    d: string
    color: string | SvgGradient
    drawStyle: "fill" | "stroke"
    fillRule?: "nonzero" | "evenodd"
    strokeWidth?: number
    strokeCap?: "butt" | "round" | "square"
    strokeJoin?: "miter" | "round" | "bevel"
  }

  /** A parsed document: intrinsic size (viewBox/width-height) plus the flat draw list. */
  export type SvgDocument = {
    width: number
    height: number
    draws: SvgDraw[]
  }

  /**
   * Parses an SVG document string into plain draw data: geometry is flattened
   * to absolute path data (every group/element transform baked in) and paints
   * are resolved to solid colors or gradients. Parsing is sandboxed: no
   * network, file, or data-URI resource access. Throws on an invalid
   * document.
   *
   * `opts.color` drives `currentColor` in the document, as a packed
   * `0xRRGGBBAA` number (alpha ignored); explicit fills/strokes still win.
   * The `@solidrt/core` re-export `parseSvg` accepts any CSS color string
   * instead and is the surface applications normally use.
   *
   * Unsupported (skipped): clipPath, masks, filters, patterns, embedded
   * images, and SVG text.
   */
  export function parseSvg(src: string, opts?: { color?: number }): SvgDocument
}
