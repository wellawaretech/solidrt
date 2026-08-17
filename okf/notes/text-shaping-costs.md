---
title: Text shaping and layout costs, measured
description: What one Impeller paragraph per wrap unit costs against one paragraph per width, and what the shared word cache changes; the numbers under the owned text engine's claims (pixel parity, cold shaping a wash, re-layout 14x cheaper, edits re-shape only their words).
created: 2026-08-17
---

# Text shaping and layout costs, measured

Cut from [text-layout-owned](../done/text-layout-owned.md) (stages 1 and
2d). Bench: `alloy/examples/text_layout_bench.rs`, release, desktop; the
sample is a 463-byte paragraph of 73 wrap units at 17px. Probe:
`text-layout-probe.tsx` at the repo root. `alloy/examples/para_metrics_probe.rs`
shows what a single-line paragraph reports.

## Shaping one paragraph per wrap unit (2026-08-16)

- Pixels: identical to `drawParagraph` on every changelog bullet and a mixed
  Latin/CJK sample (0 differing channels over the compared columns). One
  single-line paragraph per unit gives advance (`get_max_intrinsic_width`,
  includes trailing whitespace) and ink width (`get_longest_line_width`,
  excludes it) in one build, and the summed advances match Impeller's own
  placement to the sub-pixel.
- Known difference: a unit wider than the wrap width overflows whole;
  Impeller breaks inside it at grapheme level. Closed by re-splitting
  oversized units at grapheme boundaries (`overflowWrap: "anywhere"`); a
  grapheme-split word loses kerning, so a character or so fewer fits than
  Impeller's in-word break. Impeller keeps the trailing space before an
  ellipsis, ours trims it.
- Cold (text SkParagraph has never seen: first render, every real text
  change): paragraph 0.83 ms vs owned prepare 0.79 ms, a wash, because
  SkParagraph's cost is per glyph run either way.
- Re-layout at a new width from prepared runs: 0.6 us, vs a paragraph
  rebuild of 8 us when SkParagraph's global cache hits (a cold rebuild is
  the 0.83 ms). Later runs of the bench read 1.4 us vs 11 us; same ratio.
- SkParagraph keeps a global shaping cache keyed on text+style, so repeat
  builds are hits: bench cold and warm separately (unique tokens spliced
  into every word for cold).
- Objects alive per paragraph: one per wrap unit; per Text before the word
  cache, shared after it.

## The shared word cache (2026-08-17)

`alloy/src/rendertree/text/words.rs`: LRU of (word text, resolved run
style) -> (paragraph, metrics), 8192 entries, one per platform context.

- Warm prepare of the 73-unit paragraph: 13 us through the word cache vs
  230 us re-building the 73 objects through SkParagraph's own cache.
- One word edited: 32 us (one shape) vs 264 us for the whole-paragraph
  rebuild; cold text is unchanged (~1.1 ms), shaping is shaping.
- A color change misses on all its words: Impeller bakes the foreground into
  the paragraph object, so paint is part of the key. A metrics-only tier
  keyed on font fields would decouple layout from paint; not built, nothing
  measures without drawing yet.
