# Text

`<text>` is a shaped paragraph, and `<span>` is a styled run inside it. Text is
laid out by the engine's own layout and shaping, not by a browser: a paragraph
is one element that wraps, aligns and truncates as a unit.

```tsx
<text fontSize={16} maxLines={2} textOverflow="ellipsis">
  Weather for <span fontWeight={700}>Tuesday</span>
</text>
```

## Run style

The style props of a run: paragraph defaults on `<text>`, overrides on
`<span>`.

{{ decl packages/core/src/types.d.ts TextRunProps }}

The cascade is intra-paragraph only. A span inherits from its enclosing span
and then from the `<text>`; nothing inherits across the element tree, so there
is no ambient font size to chase.

`lineHeight` is the one prop with a CSS reflex worth unlearning: it is a
multiplier of `fontSize`, not a pixel value.

## text

{{ decl packages/core/src/types.d.ts TextProps }}

Paragraph-level behavior lives here: `textAlign`, `maxLines` with
`textOverflow`, `textIndent`, and `textWrap` for line-breaking quality
(`"balance"` for headings, `"pretty"` to avoid a lone last word).

An element child of a `<text>` is an inline atom, which is where the
[layout](/core/reference/layout/) props `float` and `clear` apply: a floated
atom leaves the flow and the lines it overlaps wrap around it.

## span

{{ decl packages/core/src/types.d.ts SpanProps }}

Inline only: its children are text and other spans, and it has no layout box,
which is why it is the one element with no detached form. Pointer handlers on
a span fire for the boxes its text occupies on each line it spans, and bubble
to the enclosing spans and the text.

## Line breaking in app code

A `<text>` wraps in a box. For anything else - text poured into a shape,
parting around an obstacle that is not in the flow, continued across columns,
fitted to a box by size, or placed glyph by glyph - the app does the breaking
itself over words the engine shaped once:

```tsx
let prepared = prepareText(article, { fontSize: 16, lineHeight: 1.4 })
let cursor = 0
for (let band of bands) {                        // any widths, in any order
  let line = layoutNextLine(prepared, cursor, band.w)
  if (!line) break
  lines.push({ ...band, w: line.width, text: prepared.text.slice(line.start, line.end) })
  cursor = line.cursor                            // the next box continues here
}
```

`prepareText` returns the paragraph's wrap units (a word and its trailing
whitespace) with their advance, ink width, ascent and descent. `layoutNextLine`
is a greedy breaker over those numbers: it fills one line to the width you hand
it and returns the cursor for the next, so a circle is a stack of chords, a
column is the same loop with a new box, and a headline that must fit is a loop
over sizes. Each line is drawn as a `<d-text>` of exactly its own text in the
same font.

The split is what makes it usable: shaping happened once, through the shared
word cache the drawn lines hit again, so re-breaking three paragraphs every
frame as a shape moves costs arithmetic, not layout. `carets: true` adds each
unit's per-grapheme x positions from that same shaping - the kerned source for
per-glyph placement and animation, which measuring characters one at a time
cannot give. `runs` restyles ranges, with a unit crossing a run boundary coming
back as glued pieces that always land on one line.
