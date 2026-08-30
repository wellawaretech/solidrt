// Per-glyph positions. prepareText with `carets: true` reports, per wrap unit,
// the x of every grapheme boundary from the shaping the engine draws - kerning
// included - so one d-text per glyph placed from them sits exactly on the whole
// headline. Measuring characters one at a time with measureText cannot know a
// glyph's neighbors, so pairs like AV and TA come out loose and the row drifts
// right. Three rows of the same headline: drawn whole, per glyph from carets,
// per glyph from measureText. Per-glyph d-texts are what a per-character effect
// (a wave, a color cycle, a stagger) animates.
import { measureText, prepareText, render } from "@solidrt/core"

const FONT = { fontSize: 48, fontWeight: 800 } as const
const HEADLINE = "AVATAR Two Ya"
const LEFT = 32
const TOP = 40
const ROW = 96
// A single glyph never wraps at this width; it only bounds the d-text.
const GLYPH_W = FONT.fontSize * 2
const LABEL = { fontSize: 13, color: "#8a8378" }

type Glyph = { x: number, text: string }

// Kerned: each unit's pen position plus its caret offsets.
function fromCarets(): Glyph[] {
  let prepared = prepareText(HEADLINE, { ...FONT, carets: true })
  let out: Glyph[] = []
  let pen = 0
  for (let unit of prepared.units) {
    let carets = unit.carets!
    for (let i = 0; i + 1 < carets.length; i++) {
      out.push({ x: pen + carets[i]!.x, text: prepared.text.slice(carets[i]!.offset, carets[i + 1]!.offset) })
    }
    pen += unit.advance
  }
  return out
}

// Unkerned: each character measured alone, advanced by its own width.
function fromMeasure(): Glyph[] {
  let out: Glyph[] = []
  let x = 0
  for (let ch of HEADLINE) {
    out.push({ x, text: ch })
    x += measureText(ch, FONT).width
  }
  return out
}

function Row(props: { y: number, label: string, glyphs: Glyph[], color: string }) {
  return (
    <d-view x={LEFT} y={props.y}>
      <d-text y={-20} {...LABEL}>{props.label}</d-text>
      {props.glyphs.map(g => (
        <d-text x={g.x} w={GLYPH_W} {...FONT} color={props.color}>{g.text}</d-text>
      ))}
    </d-view>
  )
}

function App() {
  return (
    <window>
      <d-rect color="#f4efe6" />
      <d-view x={LEFT} y={TOP}>
        <d-text y={-20} {...LABEL}>whole headline</d-text>
        <d-text {...FONT} color="#2b2620">{HEADLINE}</d-text>
      </d-view>
      <Row y={TOP + ROW} label="per glyph from carets: true (kerned)" glyphs={fromCarets()} color="#2b2620" />
      <Row y={TOP + 2 * ROW} label="per glyph from measureText per character (drifts)" glyphs={fromMeasure()} color="#b3452b" />
    </window>
  )
}

render(() => <App />)
