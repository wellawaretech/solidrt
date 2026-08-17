---
title: Inline styled runs in <text> via <span>
description: A paragraph cannot mix styles, so a bold lead-in or inline code is laid out a word at a time in a wrapping row; Impeller shapes styled runs natively, so expose them as <span> children of <text> plus the paragraph props the API leaves unused.
created: 2026-08-16
---

# Inline styled runs in `<text>` via `<span>`

`<text>` shapes one style for its whole content. Anything that needs a second
style mid-sentence (a bold lead-in, `inline code`, a colored label) has to be
split into separate `<text>` elements and re-flowed by hand: a wrapping flex
row of words with `columnGap`, baseline-aligned sub-rows for glued segments,
one `<text>` per segment. `scripts/changelog/changelog-shot.tsx` does exactly
this and says why in its header comment. Line breaking, baseline alignment
across sizes and justification are then flexbox's job, not the shaper's.

Impeller already does the real thing. `ParagraphBuilder` is a style stack
(`push_style` / `add_text` / `pop_style`), so a single paragraph can carry
runs of different foreground, background, family, size, weight, style,
line height, decoration and locale, and is wrapped, aligned and drawn as
one unit by `draw_paragraph`. The rendertree also already models a paragraph
as a `<text>` with child runs: Solid's universal renderer turns every JSX
text child into an internal `d-span` node (`packages/core/src/renderer.ts`
`createTextNode`), and `RenderTree::sync_text` concatenates those into
`computed_text`. The two trees are the same shape; the runs just carry no
style yet.

## Proposal

```tsx
<text color="white" fontSize={16}>
  Hello, <span color="tomato" fontWeight={700}>{name()}</span>!
  <span textDecoration={["underline"]} decorationStyle="wavy">
    nested <span fontStyle="italic">too</span>
  </span>
</text>
```

- Name: `<span>`, HTML vocabulary (Flutter: `TextSpan`). Simplified
  semantics through the solidrt lens: a span is inline text only. It may
  contain text and other spans, nothing else (throw, per the validation
  policy). No inline-block or placeholder content: Impeller's C API has none.
- A span never has a layout box, so there is no separate `d-` form; it works
  identically inside `<text>` and `<d-text>`. Internally `d-span` stays the
  leaf text run; `span` is the same kind with optional style overrides (or
  the two collapse into one kind).
- Cascade is intra-paragraph and resolved in Rust at shape time. Every span
  carries `Option<..>` overrides; `sync_text` becomes "collect runs" (walk
  `<text>` -> spans depth-first, layer overrides on the ancestor chain,
  yield `Vec<(String, EffectiveStyle)>`), and `Text::shaped` emits one
  `push_style` / `add_text` / `pop_style` per run. The rendertree keeps its
  flat per-element property writes; nothing inherits across the tree. This
  is deliberately not the same mechanism as
  [scoped-text-defaults](scoped-text-defaults.md), which is about defaults
  across the whole tree.
- `ParaKey` (the shaping cache key) includes the runs. Damage per span
  write: metrics-affecting (fontSize, fontFamily, fontWeight, fontStyle,
  lineHeight) is Layout; color, background, decoration are Paint. Nested
  span writes bubble to the owning `<text>` the way `sync_span_parent` and
  `invalidate_cache` already do for the direct child, walking up through
  span ancestors.
- Reactivity comes for free: `{name()}` inside a span is a `d-span` leaf
  that Solid updates via `replaceText`; only that run's text changes and the
  paragraph re-shapes. Whether Solid replaces the node instead of editing it
  is [text-multi-child-replacement](text-multi-child-replacement.md); spans
  make multi-child `<text>` the normal case, so do that repro as part of (or
  before) stage 2.

## Props

Per run, valid on `<span>` and (as the paragraph default) on `<text>`:

- existing: `color`, `fontFamily`, `fontSize`, `fontWeight`, `fontStyle`,
  `lineHeight`. `color` becomes a full paint (Impeller takes a `Paint` for
  the foreground), so gradient text works on both elements.
- new: `backgroundColor` (run highlight, a paint as well),
  `textDecoration: ("underline" | "overline" | "line-through")[]`,
  `decorationColor`, `decorationStyle: "solid" | "double" | "dotted" |
  "dashed" | "wavy"`, `decorationThickness` (multiplier of the font's default),
  `locale`.

Paragraph only, on `<text>` (Impeller takes these from the first pushed style
and ignores them on inner runs):

- existing: `textAlign`, `maxLines`
- new: `textOverflow: "clip" | "ellipsis" | string` (a custom ellipsis string
  is what Impeller actually takes; CSS name), `direction: "ltr" | "rtl"`.

Not reachable through the impellers 0.4.2 C surface, so explicitly out until
upstream grows them: letter and word spacing, text shadows, font features and
variation axes (which is why [font-stretch-axis](font-stretch-axis.md) waits),
strut and height behavior.

## Stages

1. Paragraph-level additions on `<text>` alone: `textOverflow`, `direction`,
   `locale`, the decoration props, `backgroundColor`, and `color` as a full
   paint. No new primitive; it exercises the properties, flux-types and docs
   plumbing that stage 2 reuses.
2. `<span>`: run collection, cascade, push/pop per run, cache key, damage
   routing, child validation, types and docs. Acceptance test: rewrite
   `changelog-shot.tsx` so a bullet is one `<text>` with span children
   (`toWords`, the wrapping row and the `Word` component go away;
   `inlineRuns` stays) and the shot looks the same or better.
3. Paragraph queries as element methods next to `getBoundingBox()`:
   `getLineMetrics()`, `getGlyphAt(x, y)`, `getWordBoundary(index)`, from
   Impeller's `get_line_metrics`, `create_glyph_info_at_paragraph_coordinates`
   / `..at_code_unit_index`, `get_word_boundary_utf16`, plus min/max
   intrinsic width. This is what text selection, caret placement in
   TextInput and shrink-to-fit need. Different consumer; can be split out
   into its own item when picked up.

## Related, not done here

- Components-level `Text` and the theme: pass span children through, or
  offer a themed `Span`. Written down only; wait for an app to need it.
- [dpi-aware-default-font-weight](dpi-aware-default-font-weight.md) touches
  the same default-style path; with runs, the default weight is the
  paragraph-level default that spans override. Whichever lands second adapts.

Alternative under evaluation: [text-layout-owned](text-layout-owned.md)
keeps this API but replaces Impeller's style stack with our own line
breaker over single-run paragraphs. If that spike fails, this item is the
path.
