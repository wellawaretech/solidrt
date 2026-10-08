// The glyph engine (gui-enabled runtime only): a registered font shaped on
// the runtime's own shaper, its glyphs rasterized into an atlas texture a
// consumer samples itself - the sprite layer's world-space text, a terminal
// grid, anything that draws glyph quads. `<text>` does not draw through
// this; it is the primitive beneath that for text the tree cannot carry.
//
// A font handle is one face (family, weight, style) with one atlas of one
// cell kind. Prepare text over it to get units whose glyphs are named and
// placed, ask for the cells of the glyphs you draw, then draw each glyph as
// a quad at its placement sampling the atlas rect. Cells are made on a
// worker thread and land at a later frame: draw a glyph whose cell has not
// arrived as nothing at its advance, and re-read the cells when the promise
// resolves.

declare module "flux:font" {
  import type { TextureId } from "flux:gpu"
  import type { TextUnit } from "flux:rendertree"

  /** A created font, valid until {@link destroyFont}. */
  type FontId = number

  /**
   * The face a font shapes and rasterizes with: the same vocabulary as a
   * `<text>`'s font props, resolved the same way (a role alias such as
   * "sans", "serif" or "mono", or a registered family name; an unknown
   * family falls back to sans). `fontSize` is the size a "mask" atlas is
   * made at and the default size {@link prepareText} shapes at; an "msdf"
   * atlas is size-independent.
   */
  type FontFace = {
    fontFamily?: "sans" | "serif" | "mono" | (string & {})
    fontSize?: number
    fontStyle?: "normal" | "italic"
    fontWeight?: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
    /** Width as a percentage of normal (CSS font-stretch), on the font's width axis; default 100. */
    fontStretch?: number
    /** Extra advance after every character in pixels (CSS letter-spacing), what {@link prepareText} shapes with by default; default 0. */
    letterSpacing?: number
    /** Line height as a multiplier of the size, 0 for the font's own line. */
    lineHeight?: number
  }

  /** Options for {@link createFont}. */
  type FontAtlasOptions = {
    /**
     * What the atlas holds. "msdf" (default): multi-channel signed distance
     * fields at `size` texels per em, one cell for every zoom; the consumer
     * decodes the field in its shader (the sprite layer's text does).
     * "mask": coverage masks at the face's pixel size, premultiplied white,
     * drawn 1:1 as a plain texture - a terminal, a HUD at a fixed size.
     */
    cells?: "mask" | "msdf"
    /** Texels per em of the cells: the face's `fontSize` for a mask atlas, 48 for an msdf one. */
    size?: number
    /** msdf only: the distance range in texels (default 8), the room an outline has. */
    range?: number
    /** Keep a mip chain on the atlas texture: default true for msdf (sampled at every zoom), false for mask. */
    mipmap?: boolean
    /**
     * mask only: how the cells are hinted. "light" snaps baseline, x-height
     * and cap height to pixel rows (`true` means the same); "full" snaps
     * stems to whole pixels as well, which a grid of cells drawn at whole
     * pixels (a terminal) wants; false (default) keeps the outline as the
     * font draws it. An msdf atlas is never hinted.
     */
    hint?: boolean | "light" | "full"
    /** Debug label of the atlas texture. */
    label?: string
  }

  /** The atlas of a font: its texture, its current size and what its cells are. */
  type FontAtlas = {
    texture: TextureId
    /** Current size in texels. The atlas grows as glyphs arrive; a change of size means EVERY cell moved, so re-read placements with {@link glyphCells}. */
    width: number
    height: number
    cells: "mask" | "msdf"
    /** Texels per em the cells were made at. */
    size: number
    /** The face's size: what {@link prepareText} shapes at by default (the runtime's text default when the face named none). */
    fontSize: number
    /** msdf: the distance range in texels; 0 for a mask atlas. */
    range: number
    /** How the cells are hinted; "off" for an msdf atlas. */
    hint: "off" | "light" | "full"
  }

  /**
   * Where a glyph's cell sits in the atlas (`x`, `y`, `width`, `height`,
   * texels) and how the cell sits on the glyph: `left` texels right of the
   * glyph origin, `top` texels above the baseline. To draw a glyph placed
   * at pen (px, py) in a text prepared at `fontSize`, scale by
   * `fontSize / atlas.size` and put the cell's top-left at
   * `(px + left * s, py + ascent - top * s)`.
   */
  type GlyphCell = {
    glyph: number
    x: number
    y: number
    width: number
    height: number
    left: number
    top: number
  }

  /** Options for {@link prepareText} over a font. */
  type FontPrepareOptions = {
    /** Pixels per em to shape at; default the face's `fontSize`. */
    fontSize?: number
    /** Line height multiplier; default the face's. */
    lineHeight?: number
    /** Extra advance after every character in pixels, at `fontSize`; default the face's. */
    letterSpacing?: number
    /** Also report each unit's caret stops (one pass over the cluster map). */
    carets?: boolean
  }

  /**
   * A wrap unit shaped on a font: `flux:rendertree`'s unit plus the unit's
   * glyphs - the font's glyph `id` (a cell of this font's atlas), the glyph
   * origin `x`/`y` from the unit's pen position (kerned, letter-spaced, y
   * down) and the pen `advance` it contributes - all in pixels at the
   * shaping size.
   */
  type FontTextUnit = TextUnit & {
    glyphs: { id: number, x: number, y: number, advance: number }[]
  }

  /** A text shaped on a font: `flux:rendertree`'s `PreparedText` with every unit carrying its glyphs. */
  type FontPreparedText = {
    text: string
    units: FontTextUnit[]
  }

  /**
   * Create a font: one face with an empty atlas. Throws on an unknown cells
   * kind or a non-positive size. The atlas texture exists from here, so a
   * layer can declare it at creation and draw from it as cells arrive.
   */
  function createFont(face?: FontFace, options?: FontAtlasOptions): FontId
  /** Free the font and its atlas texture. Requests still pending on it reject. */
  function destroyFont(font: FontId): void
  /** The font's atlas as it is now. */
  function fontAtlas(font: FontId): FontAtlas
  /**
   * Shape `text` on the font (the runtime's own shaper): the units of
   * `flux:rendertree`'s prepareText, each also carrying its glyphs, for
   * app-side line breaking with `layoutNextLine` and glyph placement.
   * The font is one face and its atlas holds that face's cells only, so a
   * character the face lacks shapes to its missing glyph (id 0) with no
   * fallback to another font, where a `<text>` would borrow the glyph;
   * whitespace and control characters the face lacks take the space's
   * advance.
   */
  function prepareText(font: FontId, text: string, options?: FontPrepareOptions): FontPreparedText
  /**
   * Make the cells of `glyphs` (ids from prepared units) that the atlas
   * does not hold yet, on the worker thread. Resolves once every glyph is
   * placed or has failed (a glyph the font cannot rasterize, or an atlas
   * at the device's texture size cap), with one {@link GlyphCell} per
   * requested glyph, null for a failed one. Work in flight: `settle`
   * waits for it. Rejects if the font is destroyed first.
   */
  function requestGlyphs(font: FontId, glyphs: number[]): Promise<(GlyphCell | null)[]>
  /** The cells of `glyphs` as the atlas holds them now, null for a glyph not (yet) in it. */
  function glyphCells(font: FontId, glyphs: number[]): (GlyphCell | null)[]
}
