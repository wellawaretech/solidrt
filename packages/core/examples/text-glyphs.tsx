// Per-glyph positions. prepareText with `carets: true` reports, per wrap unit,
// the x of every grapheme boundary from the shaping the engine draws - kerning
// included - so one detached text per glyph placed from them sits exactly on
// the whole headline. Measuring characters one at a time with measureText
// cannot know a glyph's neighbors, so pairs like AV and TA come out loose and
// the row drifts right. Three rows of the same headline: drawn whole, per
// glyph from carets, per glyph from measureText. Per-glyph detached texts are
// what a per-character effect (a wave, a color cycle, a stagger) animates.
import { measureText, prepareText, render } from "@solidrt/core"

const FONT = { fontSize: 48, fontWeight: 800 } as const
const HEADLINE = "AVATAR Two Ya"
const LEFT = 32
const TOP = 40
const ROW = 96
// A single glyph never wraps at this width; it only bounds the detached text.
const GLYPH_W = FONT.fontSize * 2
const LABEL = { fontSize: 13, color: "#8a8378" }

type Glyph = { x: number, text: string }

// Kerned: each unit's pen position plus its caret offsets.
function fromCarets(): Glyph[] {
  let prepared = prepareText(HEADLINE, { ...FONT, carets: true })
  let { first, offset, x } = prepared.carets!
  let out: Glyph[] = []
  let pen = 0
  for (let u = 0; u < prepared.length; u++) {
    // A unit's stops run from its start to its end: one glyph between each pair.
    for (let k = first[u]!; k + 1 < first[u + 1]!; k++) {
      out.push({ x: pen + x[k]!, text: prepared.text.slice(offset[k], offset[k + 1]) })
    }
    pen += prepared.advance[u]!
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
    <view detached x={LEFT} y={props.y}>
      <text detached y={-20} {...LABEL}>{props.label}</text>
      {props.glyphs.map(g => (
        <text detached x={g.x} w={GLYPH_W} {...FONT} color={props.color}>{g.text}</text>
      ))}
    </view>
  )
}

function App() {
  return (
    <window>
      <rect detached color="#f4efe6" />
      <view detached x={LEFT} y={TOP}>
        <text detached y={-20} {...LABEL}>whole headline</text>
        <text detached {...FONT} color="#2b2620">{HEADLINE}</text>
      </view>
      <Row y={TOP + ROW} label="per glyph from carets: true (kerned)" glyphs={fromCarets()} color="#2b2620" />
      <Row y={TOP + 2 * ROW} label="per glyph from measureText per character (drifts)" glyphs={fromMeasure()} color="#b3452b" />
    </window>
  )
}

render(() => <App />)
