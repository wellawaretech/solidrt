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
moving shapes), a headline sized by binary search until no word breaks,
virtualized or streamed long text broken by the app.

The temptation is to add each of these as a prop (`exclusions={[...]}`,
`float="center"`, `columns`). That builds demos into the engine. pretext
itself ships two blocks, `prepare` and `layoutNextLine(prepared, cursor,
width)`, and its editorial demo (columns with cursor handoff, circular
obstacles carved per line into slots filled left to right, drop cap and
pull quote rects, adaptive headline) is app code on top. That is the bar:
a foundation apps compose, not features apps request.

## Blocks

1. `prepare(text, style) -> handle`: the shaped runs, alloy-owned and
   cached; the deferred shared word cache (stage 2d of the owned item) is
   what makes preparing many texts and re-preparing edited ones cheap.
2. `layoutNextLine(handle, cursor, width) -> { cursor, width, height, ...
   }`: pure, one line for one width from a cursor, exactly pretext's shape;
   the internal breaker already works this way per segment.
3. Drawing a laid-out line. pretext gets this free from the DOM; we do
   not. Candidates: a `d-text` variant that takes a prepared handle plus a
   line's run range and draws the cached paragraphs at offsets (no
   reshaping), or the app renders `d-text` with the line's substring and
   the shared word cache makes that cheap. The choice decides how much of
   the owned engine's cost model reaches app code.

Not blocks: exclusion or shape props on `<text>`, a middle float, columns
on `<text>`. Those are compositions of the above and stay in app or
component code (a `<Columns>` or `<Article>` component in the components
package is the right home if one earns its keep).

## Done looks like

An app can rebuild the pretext editorial demo (columns with cursor handoff,
text on both sides of moving obstacles, drop cap, fitted headline) from
core primitives without touching alloy, at animation frame rates on
prepared text.

## Open questions

- Where the API lives: `flux:` module vs core, sync vs a native handle
  Proxy (see the isolate/native handle precedent).
- Whether `<text>` itself should be reimplemented on the exposed blocks
  (dogfooding) or keep its private path.
- Bidi and the cursor: a cursor is a logical position; visual order is the
  drawing block's problem.

## Related

- [text-layout-owned](text-layout-owned.md): the engine these blocks are
  cut from; its stages 4c, 6 and 2d come first.
- [text-inline-spans](text-inline-spans.md): the `<span>` API the runs
  carry.
