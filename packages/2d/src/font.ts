// A sprite font: the runtime's glyph engine (`flux:font`) seen as an atlas
// a layer declares plus a frame per glyph. The engine shapes the text and
// makes the glyph cells on its worker thread; this keeps the cells as
// frames, asks for the ones a run needs, and re-frames the runs whose
// glyphs arrive - including every run when the atlas grew, which moves
// every cell (a growth is a size change of `font.atlas`). The font is a
// handle from the first call: its atlas texture exists at once, so a layer
// declares it at creation and draws from it as cells land, a label's
// unbaked glyphs drawing as nothing at the right advance until then.
//
// XNA's SpriteFont, PixiJS's BitmapFont: a font asset drawn as quads. Two
// cell kinds: "msdf" (the default, what a zooming label wants), one atlas
// sharp at every zoom with the layer's fragment stage decoding the field
// (`Atlas.sdf`), and "mask", coverage at the face's exact size for drawing
// 1:1.
import { getOwner, onCleanup } from "@solidrt/core"
import type { PreparedText } from "@solidrt/core"
import { createFont, destroyFont, fontAtlas, glyphCells, prepareText, requestGlyphs } from "flux:font"
import type { FontAtlasOptions, FontFace, FontId, FontPrepareOptions, GlyphCell } from "flux:font"
import type { Atlas } from "./atlas.ts"
import type { Frame } from "./frames.ts"

/** The printable ASCII range, warmed up by default: what labels, counters
 * and HUD text are made of, so a run of them never waits for a cell. */
const ASCII_FIRST = 0x20
const ASCII_LAST = 0x7e

export type SpriteFontOptions = FontAtlasOptions & {
  /**
   * The characters to make cells for at creation (default printable
   * ASCII; `false` for none): a run drawn later from these never waits. A
   * run needing other glyphs makes them on demand and draws them when they
   * land (`ready` awaits that).
   */
  chars?: string | false
  /** Skip the owner-scoped auto-free (the core createTexture contract). */
  autoFree?: boolean
}

/** One glyph as the atlas holds it: its frame, its cell size in texels
 * and where the cell sits on the glyph (`left` right of the origin,
 * `top` above the baseline, texels). */
export type Glyph = {
  frame: Frame
  width: number
  height: number
  left: number
  top: number
}

/** What a text run registers with its font: the call that re-reads its
 * glyphs after cells landed or moved. */
export type FontListener = { _reframe(grew: boolean, arrived: ReadonlySet<number>): void }

export type SpriteFont = {
  /** The engine's handle. */
  readonly id: FontId
  /**
   * The atlas a layer declares (`createSpriteLayer([art, font.atlas])`):
   * one record for the font's life whose `width`/`height` change in place
   * as the atlas grows, `sdf` set for msdf cells.
   */
  readonly atlas: Atlas
  /** Texels per em the cells were made at: a glyph drawn at `fontSize`
   * scales its cell by `fontSize / size`. */
  readonly size: number
  /** The face's size: what `prepare` shapes at when the call names none. */
  readonly fontSize: number
  /** Shape `text` on the engine (see flux:font prepareText): every unit
   * carries its glyphs, for `layoutText`. */
  prepare(text: string, options?: FontPrepareOptions): PreparedText
  /** The glyph's frame and placement, or null while its cell is not in
   * the atlas (request it with `request`). */
  glyph(id: number): Glyph | null
  /** Ask for the cells of `ids` the atlas lacks; they land at a later
   * frame and every listening run is re-framed. */
  request(ids: Iterable<number>): void
  /** Resolves once no cells are in the making: the font is drawable as
   * asked. */
  ready(): Promise<void>
  /** Free the engine font and its atlas texture; runs over it must be
   * gone. */
  dispose(): void
  /** Internal: the runs to re-frame when cells land. */
  _listeners: Set<FontListener>
}

/**
 * Create a sprite font over a registered face (a role alias such as
 * "sans" or a registered family, the weight and style, and the size a
 * mask atlas is made at and text shapes at by default). Printable ASCII
 * is made right away; `ready()` tells when. Freed with the owning
 * reactive scope like a texture (opt out with `{ autoFree: false }`).
 */
export function createSpriteFont(face: FontFace, opts?: SpriteFontOptions): SpriteFont {
  let { chars, autoFree, ...atlasOptions } = opts ?? {}
  let id = createFont(face, { cells: "msdf", ...atlasOptions })
  let info = fontAtlas(id)
  let atlas: Atlas = { texture: info.texture, width: info.width, height: info.height }
  if (info.cells === "msdf") atlas.sdf = { range: info.range }
  let glyphs = new Map<number, Glyph>()
  let known = new Set<number>()
  // Requests in flight, and who waits for all of them to land.
  let inFlight = 0
  let waiting: (() => void)[] = []
  let disposed = false

  let toGlyph = (cell: GlyphCell): Glyph => ({
    frame: {
      texture: atlas.texture,
      u0: cell.x / atlas.width,
      v0: cell.y / atlas.height,
      u1: (cell.x + cell.width) / atlas.width,
      v1: (cell.y + cell.height) / atlas.height,
    },
    width: cell.width,
    height: cell.height,
    left: cell.left,
    top: cell.top,
  })

  // Cells landed: take the atlas's size (a change means every cell moved
  // and every frame is re-cut), record the arrivals, tell the runs.
  let landed = (ids: number[], cells: (GlyphCell | null)[]) => {
    if (disposed) return
    let now = fontAtlas(id)
    let grew = now.width !== atlas.width || now.height !== atlas.height
    atlas.width = now.width
    atlas.height = now.height
    let arrived = new Set<number>()
    if (grew) {
      let all = [...known]
      glyphs.clear()
      glyphCells(id, all).forEach((cell, i) => {
        if (cell !== null) glyphs.set(all[i]!, toGlyph(cell))
      })
    }
    cells.forEach((cell, i) => {
      // A failed glyph (null) stays absent: drawn as nothing at its advance.
      if (cell !== null) glyphs.set(ids[i]!, toGlyph(cell))
      arrived.add(ids[i]!)
    })
    for (let listener of font._listeners) listener._reframe(grew, arrived)
  }

  let font: SpriteFont = {
    id,
    atlas,
    size: info.size,
    fontSize: info.fontSize,
    _listeners: new Set(),
    prepare(text, options) {
      if (disposed) throw new Error("prepare: the sprite font is disposed")
      return prepareText(id, text, options)
    },
    glyph(glyph) {
      return glyphs.get(glyph) ?? null
    },
    request(ids) {
      if (disposed) return
      let missing: number[] = []
      for (let glyph of ids) {
        if (!known.has(glyph)) {
          known.add(glyph)
          missing.push(glyph)
        }
      }
      if (missing.length === 0) return
      inFlight++
      requestGlyphs(id, missing)
        .then(cells => landed(missing, cells))
        .catch(error => console.warn(`[2d] glyph request failed: ${error}`))
        .finally(() => {
          inFlight--
          if (inFlight === 0) {
            let resolve = waiting
            waiting = []
            for (let r of resolve) r()
          }
        })
    },
    ready() {
      if (inFlight === 0) return Promise.resolve()
      return new Promise(resolve => waiting.push(resolve))
    },
    dispose() {
      if (disposed) return
      disposed = true
      font._listeners.clear()
      destroyFont(id)
    },
  }
  if (chars !== false) {
    let warm = chars ?? ascii()
    let ids = new Set<number>()
    for (let unit of font.prepare(warm).units) for (let g of unit.glyphs ?? []) ids.add(g.id)
    font.request(ids)
  }
  if (autoFree !== false && getOwner()) onCleanup(() => font.dispose())
  return font
}

function ascii(): string {
  let out = ""
  for (let c = ASCII_FIRST; c <= ASCII_LAST; c++) out += String.fromCharCode(c)
  return out
}
