// Tests for the text layout (text-layout.ts): glyph placements from
// hand-built prepared units (what a sprite font's prepare returns),
// pinning the greedy breaking against core's layoutNextLine rules, the
// alignment and anchors and the throws. Pure-module input only, so it
// runs headless on flux: `sol test packages/2d`.

import { expect, test } from "flux:test"
import type { FontPreparedText, FontTextUnit } from "flux:font"
import { layoutText } from "../src/text-layout.ts"

// A monospace stand-in: every glyph advances by ADVANCE, a unit's width is
// its glyph count times that, a trailing space adds one more advance.
const ADVANCE = 10
const ASCENT = 8
const DESCENT = 2

// `text` as units: words split at spaces, each unit's advance including
// its trailing space, glyph ids the char codes, pen positions per glyph.
function prepare(text: string): FontPreparedText {
  let units: FontTextUnit[] = []
  let at = 0
  for (let line of text.split("\n")) {
    let words = line.split(" ")
    for (let [i, word] of words.entries()) {
      let last = i === words.length - 1
      let glyphs = [...word].map((ch, j) => ({ id: ch.charCodeAt(0), x: j * ADVANCE, y: 0, advance: ADVANCE }))
      let width = word.length * ADVANCE
      let advance = width + (last ? 0 : ADVANCE)
      let end = at + word.length + (last ? 0 : 1)
      units.push({
        text: word,
        start: at,
        end,
        advance,
        width,
        ascent: ASCENT,
        descent: DESCENT,
        hardBreak: last,
        glue: false,
        glyphs,
      })
      at = end
    }
    at += 1
  }
  // The last unit's hard break is the text's end, not a break.
  units[units.length - 1]!.hardBreak = false
  return { text, units }
}

test("a single line places every glyph at its pen position on one baseline", () => {
  let layout = layoutText(prepare("ab cd"))
  expect(layout.lines.length).toBe(1)
  expect(layout.glyphs.map(g => g.x)).toEqual([0, 10, 30, 40])
  expect(layout.glyphs.every(g => g.y === ASCENT)).toBe(true)
  expect(layout.width).toBe(50)
  expect(layout.height).toBe(ASCENT + DESCENT)
  expect(layout.ascent).toBe(ASCENT)
})

test("a hard break starts a new line one line box down", () => {
  let layout = layoutText(prepare("ab\ncd"))
  expect(layout.lines.length).toBe(2)
  expect(layout.lines[1]!.y).toBe(2 * ASCENT + DESCENT)
  expect(layout.glyphs.map(g => [g.x, g.y])).toEqual([
    [0, ASCENT],
    [10, ASCENT],
    [0, 2 * ASCENT + DESCENT],
    [10, 2 * ASCENT + DESCENT],
  ])
  expect(layout.height).toBe(2 * (ASCENT + DESCENT))
})

test("maxWidth wraps greedily at unit boundaries, an overlong unit goes whole", () => {
  // "aaa bb c": the inks are 30, 20 and 10 with a 10 px space after the
  // first two, so "aaa bb" is 60 wide and the whole line 80.
  let layout = layoutText(prepare("aaa bb c"), { maxWidth: 65 })
  expect(layout.lines.map(l => [l.from, l.to])).toEqual([
    [0, 2],
    [2, 3],
  ])
  expect(layout.lines[0]!.width).toBe(60)
  // One pixel less than "aaa bb" needs breaks after the first unit.
  expect(layoutText(prepare("aaa bb c"), { maxWidth: 59 }).lines.map(l => [l.from, l.to])).toEqual([
    [0, 1],
    [1, 3],
  ])
  // A unit wider than the line is placed whole and overflows.
  let wide = layoutText(prepare("abcdefgh"), { maxWidth: 30 })
  expect(wide.lines.length).toBe(1)
  expect(wide.width).toBe(80)
})

test("lines align inside the run's width", () => {
  let right = layoutText(prepare("abcd\nab"), { align: "right" })
  expect(right.lines[1]!.x).toBe(20)
  expect(right.glyphs[4]!.x).toBe(20)
  let center = layoutText(prepare("abcd\nab"), { align: "center" })
  expect(center.lines[1]!.x).toBe(10)
})

test("anchors shift the whole run", () => {
  let text = prepare("abcd")
  expect(layoutText(text, { anchor: "middle" }).glyphs[0]!.x).toBe(-20)
  expect(layoutText(text, { anchor: "end" }).glyphs[0]!.x).toBe(-40)
  expect(layoutText(text, { anchorY: "baseline" }).glyphs[0]!.y).toBe(0)
  expect(layoutText(text, { anchorY: "middle" }).glyphs[0]!.y).toBe(ASCENT - (ASCENT + DESCENT) / 2)
  expect(layoutText(text, { anchorY: "bottom" }).glyphs[0]!.y).toBe(-DESCENT)
})

test("an explicit line height sets the baseline step", () => {
  let layout = layoutText(prepare("a\nb\nc"), { lineHeight: 30 })
  expect(layout.lines.map(l => l.y)).toEqual([ASCENT, ASCENT + 30, ASCENT + 60])
  expect(layout.height).toBe(90)
})

test("a non-positive wrap width throws", () => {
  expect(() => layoutText(prepare("ab"), { maxWidth: 0 })).toThrow()
})

test("an empty text is one empty line of no size", () => {
  let layout = layoutText({ text: "", units: [] })
  expect(layout.glyphs.length).toBe(0)
  expect(layout.lines.length).toBe(1)
  expect([layout.width, layout.height]).toEqual([0, 0])
})
