// App-side line breaking. prepareText shapes a paragraph's words once (through
// the shared word cache) into units carrying advance, ink width, ascent and
// descent; layoutNextLine then breaks one line out of them per call, at
// whatever width it is handed, and returns the cursor the next call continues
// from. The engine never sees a "shape" or a "column": here the paragraph is
// poured band by band into a circle (each band's width is the chord at that
// height, its line centered on it) and, when the circle is full, the same
// cursor continues in a column beside it. Every line is a d-text of exactly its
// own text, so the whole break redoes on every resize for arithmetic plus
// word-cache hits; nothing is shaped twice.
import { For, createMemo, layoutNextLine, prepareText, render, windowSize } from "@solidrt/core"

const FONT = { fontSize: 16, lineHeight: 1.4 }
const PAD = 32
// Gap between the circle and the column that continues its text.
const GAP = 32
// A chord shorter than this holds no word: skip the band at the circle's poles.
const MIN_BAND = 90
// Breathing room between the ring and the line ends.
const INSET = 10

const TEXT =
  "A word is shaped once and keeps its width. From there a line is a loop: add words while the pen stays " +
  "inside the width you were given, stop, and hand the cursor to whoever wants the next line. Give each " +
  "call the chord of a circle and the paragraph fills the circle; give the next call a column and it " +
  "continues there, because a column is only the same loop in a new box. The engine shaped the words " +
  "before the first frame and never hears about circles or columns at all. Resize the window: the circle " +
  "grows, every band changes width, and the text re-breaks into it while the words stay as they were. " +
  "That is the whole trick, and it is the oldest one in typesetting: a compositor with a case of type " +
  "never reshaped a letter when the column narrowed. The letters kept their width and only the breaks " +
  "moved. Text engines lost that when a paragraph became one opaque object laid out at one width, and " +
  "every layout question turned into a re-shape. Prepared text gives the idea back, and this page is that " +
  "loop and nothing else."

type Placed = { x: number, y: number, w: number, text: string }

function App() {
  let prepared = prepareText(TEXT, FONT)
  let lines = createMemo<Placed[]>(() => {
    let { width, height } = windowSize()
    let r = Math.min(width * 0.24, (height - 2 * PAD) / 2)
    let cx = PAD + r
    let cy = height / 2
    let units = prepared.units
    // Single style: every line is as tall as its first unit.
    let lineH = units[0]!.ascent + units[0]!.descent
    let out: Placed[] = []
    let cursor = 0
    // The circle, top to bottom: each band's width is the chord at its middle.
    for (let y = cy - r; y + lineH <= cy + r && cursor < units.length; y += lineH) {
      let dy = y + lineH / 2 - cy
      let half = Math.sqrt(Math.max(0, (r - INSET) * (r - INSET) - dy * dy))
      if (2 * half < MIN_BAND) continue
      let line = layoutNextLine(prepared, cursor, 2 * half)
      if (!line) break
      out.push({ x: cx - line.width / 2, y, w: line.width, text: prepared.text.slice(line.start, line.end) })
      cursor = line.cursor
    }
    // The column beside it continues from the same cursor.
    let colX = cx + r + GAP
    let colW = width - colX - PAD
    for (let y = PAD; y + lineH <= height - PAD && cursor < units.length; y += lineH) {
      let line = layoutNextLine(prepared, cursor, colW)
      if (!line) break
      out.push({ x: colX, y, w: line.width, text: prepared.text.slice(line.start, line.end) })
      cursor = line.cursor
    }
    return out
  })
  let circle = createMemo(() => {
    let { width, height } = windowSize()
    let r = Math.min(width * 0.24, (height - 2 * PAD) / 2)
    return { x: PAD, y: height / 2 - r, d: 2 * r }
  })
  return (
    <window>
      <d-rect color="#f4efe6" />
      <d-oval x={circle().x} y={circle().y} w={circle().d} h={circle().d} drawStyle="stroke" strokeWidth={1} color="#d8b26a" />
      <For each={lines()} keyed={false}>
        {line => (
          <d-text x={line().x} y={line().y} w={line().w + 1} {...FONT} color="#2b2620">
            {line().text}
          </d-text>
        )}
      </For>
    </window>
  )
}

render(() => <App />)
