// Text layout, pure: the units of a text prepared on the glyph engine
// (every unit carrying its glyphs) to one placement per glyph, in run
// pixels from the run's anchor point, plus the lines and the run's box.
// The line breaking is core's `layoutNextLine` (greedy over wrap units,
// glued pieces kept together, a hard break ending a line, an overlong
// unit placed whole), restated here rather than imported because core
// loads the gui modules and this file is checked headless on the flux
// binary (tests/text-layout.test.ts), like frames.ts. No GPU or GUI
// imports: the flux:font import is a type. Letter spacing is the
// engine's, shaped into the units (`prepareText`'s `letterSpacing`).
import type { FontPreparedText, FontTextUnit } from "flux:font"

/** The anchor of the run's x: where the point sits on each line's
 * extent (SVG's text-anchor, d-text's `anchor`). */
export type TextAnchor = "start" | "middle" | "end"
/** The anchor of the run's y: the top of the first line, the middle of
 * the box, the first baseline, or the bottom of the last line. */
export type TextAnchorY = "top" | "middle" | "baseline" | "bottom"
/** How lines sit inside the run's width when they differ (a wrapped or
 * multi-line run). */
export type TextAlign = "left" | "center" | "right"

export type TextLayoutOptions = {
  /** Wrap at this width in run pixels; unset, only `\n` breaks. */
  maxWidth?: number
  /** Line alignment inside the run's width; default "left". */
  align?: TextAlign
  /** Default "start". */
  anchor?: TextAnchor
  /** Default "top". */
  anchorY?: TextAnchorY
  /** Baseline to baseline, run pixels; default each line's own ascent
   * plus descent. */
  lineHeight?: number
}

/** One glyph to draw: the font's glyph id and its origin (pen x, baseline
 * y) in run pixels from the anchor point. */
export type GlyphPlacement = { id: number; x: number; y: number }

/** One laid-out line. */
export type TextLine = {
  /** Unit range [from, to) into the prepared units. */
  from: number
  to: number
  /** The line's x (its start after alignment) and baseline y, from the anchor point. */
  x: number
  y: number
  /** Ink width (advances plus the last unit's width, trailing whitespace off). */
  width: number
  ascent: number
  descent: number
}

export type TextLayout = {
  glyphs: GlyphPlacement[]
  lines: TextLine[]
  /** The run's box: the widest line and the lines stacked. */
  width: number
  height: number
  /** The first line's ascent: the first baseline below the box's top. */
  ascent: number
}

/**
 * Lay out a prepared text (from `SpriteFont.prepare`) into glyph
 * placements.
 */
export function layoutText(prepared: FontPreparedText, opts?: TextLayoutOptions): TextLayout {
  let units = prepared.units
  let maxWidth = opts?.maxWidth ?? Infinity
  if (!(maxWidth > 0)) throw new Error(`layoutText: maxWidth must be positive, got ${maxWidth}`)
  // Break into lines: the greedy pass, lines as unit ranges.
  type Range = { from: number; to: number; ascent: number; descent: number; width: number }
  let ranges: Range[] = []
  let cursor = 0
  while (cursor < units.length) {
    ranges.push(nextLine(units, cursor, maxWidth))
    cursor = ranges[ranges.length - 1]!.to
  }
  // A text with no units (empty) still has one empty line of no height.
  if (ranges.length === 0) ranges.push({ from: 0, to: 0, ascent: 0, descent: 0, width: 0 })
  let runWidth = 0
  for (let r of ranges) runWidth = Math.max(runWidth, r.width)
  let lineHeight = opts?.lineHeight
  // Stack the lines: each baseline is its ascent below the line's top.
  let lines: TextLine[] = []
  let top = 0
  let align = opts?.align ?? "left"
  for (let r of ranges) {
    let slack = runWidth - r.width
    let x = align === "center" ? slack / 2 : align === "right" ? slack : 0
    let height = lineHeight ?? r.ascent + r.descent
    lines.push({ from: r.from, to: r.to, x, y: top + r.ascent, width: r.width, ascent: r.ascent, descent: r.descent })
    top += height
  }
  let height = top
  // The anchor point's offset: what every x and y shifts by.
  let anchor = opts?.anchor ?? "start"
  let anchorY = opts?.anchorY ?? "top"
  let dx = anchor === "middle" ? -runWidth / 2 : anchor === "end" ? -runWidth : 0
  let firstAscent = lines[0]!.ascent
  let dy = anchorY === "middle" ? -height / 2 : anchorY === "baseline" ? -firstAscent : anchorY === "bottom" ? -height : 0
  let glyphs: GlyphPlacement[] = []
  for (let line of lines) {
    line.x += dx
    line.y += dy
    let pen = line.x
    for (let i = line.from; i < line.to; i++) {
      let unit = units[i]!
      for (let g of unit.glyphs) glyphs.push({ id: g.id, x: pen + g.x, y: line.y + g.y })
      pen += unit.advance
    }
  }
  return { glyphs, lines, width: runWidth, height, ascent: firstAscent }
}

// Ink width of the wrap unit starting at `index` (core's unitInk): its
// advance through every glued piece after it, plus the last piece's ink.
function unitInk(units: FontTextUnit[], index: number): number {
  let ink = units[index]!.width
  let advance = 0
  for (let j = index + 1; j < units.length && units[j]!.glue; j++) {
    advance += units[j - 1]!.advance
    ink = advance + units[j]!.width
  }
  return ink
}

// The next line from unit `cursor` that fits `width` (core's
// layoutNextLine): units go on while the pen plus the unit's ink stays
// within the width; a hard break ends the line; a unit wider than the
// line on its own goes on whole.
function nextLine(units: FontTextUnit[], cursor: number, width: number) {
  let pen = 0
  let ascent = 0
  let descent = 0
  let i = cursor
  while (i < units.length) {
    let unit = units[i]!
    if (i > cursor && !unit.glue && pen + unitInk(units, i) > width) break
    pen += unit.advance
    if (unit.ascent > ascent) ascent = unit.ascent
    if (unit.descent > descent) descent = unit.descent
    i++
    if (unit.hardBreak) break
  }
  let last = units[i - 1]!
  // The ink width: trailing whitespace off, like the unit's own `width`.
  let lineWidth = pen - last.advance + last.width
  return { from: cursor, to: i, ascent, descent, width: Math.max(lineWidth, 0) }
}
