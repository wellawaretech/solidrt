// Prepared text: prepareText's columns, and breakLines on the <text>
// breaker against layoutNextLine's one-line-at-a-time loop (the same greedy
// rule, so the same lines).

import { test, expect } from "@solidrt/test"
import { breakLines, layoutNextLine, prepareText } from "@solidrt/core"
import type { MeasureTextOptions, PreparedText } from "@solidrt/core"

const FONT: MeasureTextOptions = { fontSize: 16, lineHeight: 1.3 }
const PARAGRAPH =
  "Measure every word once, then decide where the lines go by arithmetic.\n" +
  "A compositor does not reshape a letter when the column narrows; only the breaks move. " +
  "Antidisestablishmentarianism overflows a narrow line on its own.\n\n" +
  "A blank line above, and a last line without a break."
// Widths that wrap the paragraph differently: narrower than its longest
// word, a few words, most of a line, and no wrapping at all.
const WIDTHS = [40, 97, 180, 333.3, Infinity]

// The lines layoutNextLine gives for the whole text at one width.
function loop(prepared: PreparedText, width: number) {
  let lines = []
  let cursor = 0
  let line
  while ((line = layoutNextLine(prepared, cursor, width))) {
    lines.push(line)
    cursor = line.cursor
  }
  return lines
}

test("prepareText returns one column per unit field", () => {
  let prepared = prepareText("two words\nthree", FONT)
  expect(prepared.length).toBe(3)
  expect(Array.from(prepared.start)).toEqual([0, 4, 10])
  expect(Array.from(prepared.end)).toEqual([4, 10, 15])
  expect(Array.from(prepared.hardBreak)).toEqual([0, 1, 0])
  expect(Array.from(prepared.glue)).toEqual([0, 0, 0])
  expect(prepared.advance[0]!).toBeGreaterThan(prepared.width[0]!)
  expect(prepared.run).toBe(undefined)
  expect(prepared.carets).toBe(undefined)
})

test("runs add the run column and glue pieces of a unit that crosses one", () => {
  let text = "plain bolder tail"
  let prepared = prepareText(text, { ...FONT, runs: [{ start: 6, end: 10, fontWeight: 700 }] })
  // "bold" is the run, "er " continues its wrap unit in the base font.
  expect(Array.from(prepared.run!)).toEqual([-1, 0, -1, -1])
  expect(Array.from(prepared.glue)).toEqual([0, 0, 1, 0])
})

test("carets are one table indexed per unit", () => {
  let prepared = prepareText("ab cd", { ...FONT, carets: true })
  let { first, offset, x } = prepared.carets!
  expect(Array.from(first)).toEqual([0, 4, 7])
  // "ab ": a stop at each grapheme boundary, absolute offsets.
  expect(Array.from(offset)).toEqual([0, 1, 2, 3, 3, 4, 5])
  expect(x[0]).toBe(0)
  expect(x[3]).toBeCloseTo(prepared.advance[0]!, 4)
})

test("breakLines breaks where layoutNextLine does", () => {
  let prepared = prepareText(PARAGRAPH, FONT)
  for (let width of WIDTHS) {
    let expected = loop(prepared, width)
    let lines = breakLines(prepared, width)
    expect(lines.length).toBe(expected.length)
    expect(Array.from(lines.from)).toEqual(expected.map(l => l.from))
    expect(Array.from(lines.to)).toEqual(expected.map(l => l.to))
    expect(Array.from(lines.hardBreak)).toEqual(expected.map(l => (l.hardBreak ? 1 : 0)))
    expect(Array.from(lines.width)).toBeCloseTo(expected.map(l => l.width), 3)
    expect(Array.from(lines.height)).toBeCloseTo(expected.map(l => l.height), 3)
    expect(Array.from(lines.ascent)).toBeCloseTo(expected.map(l => l.ascent), 3)
    let top = 0
    let tops = expected.map(l => {
      let y = top
      top += l.height
      return y
    })
    expect(Array.from(lines.y)).toBeCloseTo(tops, 3)
    expect(lines.totalHeight).toBeCloseTo(top, 3)
  }
})

test("breakLines balances on request, keeping the line count", () => {
  let prepared = prepareText("one two three four five six seven", FONT)
  let width = breakLines(prepared, Infinity).width[0]! * 0.7
  let greedy = breakLines(prepared, width)
  let balanced = breakLines(prepared, width, { textWrap: "balance" })
  expect(balanced.length).toBe(greedy.length)
  let widest = (lines: { width: Float32Array }) => Math.max(...lines.width)
  expect(widest(balanced)).toBeLessThan(widest(greedy))
})

test("breakLines takes columns built by hand", () => {
  let lines = breakLines(
    {
      text: "ab cd",
      length: 2,
      start: Uint32Array.of(0, 3),
      end: Uint32Array.of(3, 5),
      advance: Float32Array.of(30, 20),
      width: Float32Array.of(20, 20),
      ascent: Float32Array.of(8, 8),
      descent: Float32Array.of(2, 2),
      hardBreak: Uint8Array.of(0, 0),
      glue: Uint8Array.of(0, 0),
    },
    45,
  )
  expect(Array.from(lines.to)).toEqual([1, 2])
  expect(Array.from(lines.y)).toEqual([0, 10])
})

test("breakLines throws on a bad width, columns or textWrap", () => {
  let prepared = prepareText("ab cd", FONT)
  expect(() => breakLines(prepared, -1)).toThrow("width")
  expect(() => breakLines(prepared, NaN)).toThrow("width")
  expect(() => breakLines({ ...prepared, advance: new Float32Array(1) }, 10)).toThrow("prepared.advance")
  expect(() => breakLines({ ...prepared, glue: new Float32Array(prepared.length) as never }, 10)).toThrow("prepared.glue")
  expect(() => breakLines(prepared, 10, { textWrap: "even" as never })).toThrow("textWrap")
})

test("prepareText throws on invalid options", () => {
  expect(() => prepareText("a", { fontSize: "big" as never })).toThrow("fontSize")
  expect(() => prepareText("abc", { runs: [{ start: 2, end: 1 }] })).toThrow("runs")
})

test("an empty text prepares and breaks to nothing", () => {
  let prepared = prepareText("", FONT)
  expect(prepared.length).toBe(0)
  expect(breakLines(prepared, 100).length).toBe(0)
})
