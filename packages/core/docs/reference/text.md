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
