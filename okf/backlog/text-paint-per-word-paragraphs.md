---
title: A paragraph is painted as one Impeller paragraph per word, an order of magnitude over what its glyphs need
description: Five 230-character paragraphs in reflowing panes cost the Galaxy Tab A7 ~10 ms of layout, ~12 ms of paint recording and ~12 ms of GPU per frame, for ~1100 glyphs. The word cache keeps every wrap unit as its own Impeller Paragraph and paint emits draw_paragraph per word (~180 ops a frame for that text), so recording, display-list processing and the GPU's text draws all scale with word count; a per-line draw, or glyph runs, would cut them ~8x.
created: 2026-09-22
---

# A paragraph is painted as one Impeller paragraph per word, an order of magnitude over what its glyphs need

## Symptom

demo/notes on the Galaxy Tab A7 (SM-T500, Adreno 610, release client built
2026-09-22 with the texture radius and the cadence idle reset). Ten panes
sliding and resizing on a layout transition, five of them holding a
`d-text` paragraph of ~230 characters in a fixed-size box (the box does not
change during the slide). Stepped add, 60 frames, get_stats window_frames:

| paragraphs | JS critical path p50 / p95 | worst frame | slow frames of 60 |
|---|---|---|---|
| none (empty notes) | 6.9 / 12.7 ms | layout 1.6, paint 14.3 | 0 |
| five, drawn live | 12.4 / 20.4 ms | layout 10.0, paint 12.4, wordHits 157 | 9 |
| five, each box a `snapshot-no-aa` boundary | 6.9 / 13.4 ms | layout 3.8, paint 10.6 | 0 |

Compositor GPU span (frameReady minus queue) rose from ~21 to ~33 ms per
frame with the five live paragraphs, back to ~25 with the boundaries (all
under a hold of 3, so comparable to each other only). Five paragraphs are
about 1100 glyphs: a batched glyph-run draw of that size is well under a
millisecond of GPU and of recording on this class of device.

The boundary is the app-side workaround and it has its own price: the first
frame of each reflow re-rasterizes every paragraph at its new size, 57 ms
once with five paragraphs (paint 53.9 on that frame), and an entering or
exiting pane scales its paragraph's bitmap.

## Where it goes

- text/words.rs: the word cache holds each wrap unit as a single-line
  Impeller `Paragraph` (shaped once, shared - the right call for shaping).
- text/mod.rs `Text::build`: for every placed run, `words.get_or_shape`
  then `builder.draw_paragraph(&word.paragraph, ...)`. A 230-character
  paragraph is ~36 words, so five paragraphs put ~180 paragraph draws into
  the display list per frame (wordHits 157 above is that lookup count).
- Each of those is a display-list op carrying an Impeller paragraph: the
  raster thread's per-op cost (okf/backlog/display-list-op-cost.md) and
  Impeller's per-paragraph text pipeline (glyph collection against the
  atlas, one text draw per paragraph, nothing batched across them) both
  scale with word count, not glyph count. The layout side re-runs
  `owned_layout` per frame as well (layout 10 ms with a fixed width; the
  per-width line cache should make that free - worth checking why not).

## What done looks like

- Paint emits one draw per line (or per run of same-styled words on a
  line), not per word: a line-level Impeller paragraph cached per (line
  text, style), built from the word cache's metrics, or glyph runs
  positioned from the shaped words if the binding exposes them. Recording
  and GPU text cost then scale with lines.
- A fixed-width paragraph in a moving ancestor costs no layout per frame:
  the per-width line layout is reused when neither text, style nor width
  changed.
- Measured on the same demo: five live paragraphs in reflowing panes at
  60 fps with no boundary, and the boundary no longer needed for text.
