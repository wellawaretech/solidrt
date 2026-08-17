---
title: Text layout primitives for apps
description: Expose the owned text layout's building blocks (prepare, next line for a width from a cursor, draw a laid-out line) to app code, so editorial layouts (column handoff, obstacles, fitted headlines) are app work on a stable foundation instead of ever more <text> props.
created: 2026-08-17
---

# Text layout primitives for apps

## Problem

[text-layout-owned](text-layout-owned.md) gives alloy a pretext-style
engine: runs measured once, lines broken by arithmetic, a per-line seam
(`text_layout::layout`'s extent hook) that says which spans a line may use.
Everything an app can do with it, though, is behind `<text>` props
(`textIndent`, `float`, `maxLines`, ...). An app cannot ask "the next line
for width w from cursor c" and cannot draw a line it laid out itself, so
anything the props do not name is unreachable: text handed from column to
column at a cursor, text around obstacles the app owns (positioned or
moving shapes), a headline sized by binary search until no word breaks.

The temptation is to add each of these as a prop (`exclusions={[...]}`,
`float="center"`, `columns`). That builds demos into the engine. pretext
itself ships two blocks, `prepare` and `layoutNextLine(prepared, cursor,
width)`, and its editorial demo (columns with cursor handoff, circular
obstacles carved per line into slots filled left to right, drop cap and
pull quote rects, adaptive headline) is app code on top. That is the bar:
a foundation apps compose, not features apps request.

## Who this is for (decided 2026-08-17)

A power-user API for the non-standard case: small texts laid out by
bespoke app code (a headline into a shape, prose around a moving thing,
column handoff, fitted type). It is not the text engine: regular text of
any length, including long or streamed text an app windows into chunks,
goes through `<text>`, where the cost model lives (word cache, per-node
layout cache, atoms, floats). Consequences: the shapes are plain objects a
person reads and writes (`u.advance`, `text.slice(u.start, u.end)`,
destructure, log, filter), not typed arrays; per-unit records carry
everything the engine knows rather than the minimum; hitting a throughput
wall with the primitive means a `<text>` feature is missing, not that the
primitive needs a faster shape.

## Blocks

1. `prepareText(text, options) -> PreparedText` (flux:rendertree, next to
   `measureText`, same font options; single style like pretext's
   `prepare(text, font)`). Native segments (UAX 14) and shapes every unit
   through the shared word cache; returns plain data, no native handle
   (nothing to dispose or keep alive across reloads, layout runs in JS with
   no native calls per line):
   `{ text, units: TextUnit[] }`, `TextUnit = { text, start, end, advance,
   width, ascent, descent, hardBreak }` with `start`/`end` UTF-16 offsets
   (JS indexing), `advance` with trailing whitespace, `width` the ink.
2. `layoutNextLine(prepared, cursor, width) -> Line | null` (core, pure
   JS): greedy, units fit while sum(advance before) + width(last) <= width;
   a unit wider than the line alone overflows whole; a hard break ends the
   line. `Line = { from, to, start, end, width, height, ascent, hardBreak,
   cursor }`. Deliberately the simple breaker: no floats, balance or
   ellipsis at this level; a power user can ignore it and break from the
   units directly.
3. Drawing: the existing `<d-text x y w>` with the line's substring and the
   same font options; the word cache makes each line's words hits, and
   drawing the exact substring unwrapped (`w` just over its width) avoids
   depending on the engine re-breaking identically. What pretext gets from
   the DOM. A draw-from-prepared primitive only if per-line text nodes
   prove too costly in the demo.

Not blocks: exclusion or shape props on `<text>`, a middle float, columns
on `<text>`. Those are compositions of the above and stay in app or
component code (a `<Columns>` or `<Article>` component in the components
package is the right home if one earns its keep).

## Stage 1 (DONE 2026-08-17)

Blocks 1-3 as decided above. `alloy::rendertree::text::prepare_units`
(shape.rs; segments + word cache, byte offsets), `prepareText` in
`flux:rendertree` (tree.rs, next to `measureText`; shares the font option
parsing, which gained `lineHeight`; converts offsets to UTF-16),
`prepareText`/`layoutNextLine`/`TextLine` in `@solidrt/core` (core.ts),
types in flux-types, docs/core.md "prepareText and layoutNextLine".
`examples/text-flow`: two columns with cursor handoff, a pointer-driven
circle carved per line into a left and a right slot, a drop cap reserving
the first lines; verified live (reflows on pointer moves). Per-line
`<d-text>` drawing was fine at this scale; no draw-from-prepared primitive.

## Done looks like

An app can rebuild the pretext editorial demo (columns with cursor handoff,
text on both sides of moving obstacles, drop cap, fitted headline) from
core primitives without touching alloy, at animation frame rates on
prepared text.

## Deliberately not done (until asked)

- Whether `<text>` itself should be reimplemented on the exposed blocks
  (dogfooding) or keep its private path (it keeps it: the blocks are the
  power-user surface, `<text>` is the engine).
- Bidi and the cursor: a cursor is a logical position; visual order is the
  drawing block's problem.
- Styled runs in a prepared text (spans), if a bespoke layout ever needs
  mixed styles; single style until then.
- Fitted headlines want a fast "does it fit in N lines" loop; today that is
  prepare per size (word cache hits) plus layoutNextLine.

## Related

- [text-layout-owned](text-layout-owned.md): the engine these blocks are
  cut from; its stages 4c, 6 and 2d landed first (2026-08-17).
- [text-inline-spans](text-inline-spans.md): the `<span>` API the runs
  carry.
- [text-bidi](../backlog/text-bidi.md): the cursor stays logical; visual
  order is the placer's job when bidi lands.
