---
title: Hyphenation and optimal-fit line breaking
description: Justified narrow columns show lines with huge word gaps when the next word is long, and textWrap="pretty" only rescues a lone last word; TeX solves both with hyphenation plus a whole-paragraph optimal-fit breaker (Knuth-Plass), Safari's text-wrap pretty does the same, and now that the breaker is ours it is bounded engine work.
created: 2026-08-17
---

# Hyphenation and optimal-fit line breaking

## Symptom

`<text textAlign="justify">` in a column of ~35 characters (the
`examples/text-flow/src/index2.tsx` page at a narrow window) produces lines
like "that idea when whole" with the slack spread over three gaps because
the next word ("paragraphs") does not fit. Ragged text has the milder form:
one very short line before a long word. `textWrap="pretty"` does not help;
it only pulls one word down when the last line is a lone word.

## Why

The breaker is greedy: it fills each line as far as it can and never looks
ahead, and it can only break between wrap units, so a long word has no
break candidate inside it. Every greedy justifier does this; what tames it
is (a) more break candidates (hyphenation) and (b) choosing breaks for the
whole paragraph rather than line by line (optimal fit). TeX has done both
since 1982 (Knuth-Plass over Liang's hyphenation patterns); Safari's
`text-wrap: pretty` is a Knuth-Plass over the paragraph; Chrome's is a
score-based lookahead over the last few lines. Ours is a lone-word rescue.
Behind Impeller's paragraph none of this was reachable; with the owned
breaker ([text-layout-owned](../done/text-layout-owned.md)) it is arithmetic
over units that already carry advance, ink width and hard breaks.

## Done looks like

1. Hyphenation: a unit that does not fit is split at a hyphenation point
   (Liang patterns via the `hyphenation` crate, language from a `lang`
   prop or the app locale, en-US shipped, others loadable), the halves
   re-shaped through the existing grapheme re-split path (the first half
   with a trailing hyphen glyph, so its ink width is real), and placed as
   two units. Off by default like CSS `hyphens: manual`; `hyphens="auto"`
   on `<text>` turns it on. Soft hyphens (U+00AD) honored as manual break
   points regardless.
2. Optimal fit: `textWrap="pretty"` becomes a whole-paragraph Knuth-Plass
   (minimize total badness = summed squared looseness, penalties for
   hyphenated lines, consecutive hyphens, and a lone last word), over the
   same units and per-line extents the greedy breaker uses, so floats,
   indent and segments still hold. Greedy stays the default (`"wrap"`) and
   `"balance"` stays as is. Cost bounded: paragraphs are hundreds of units;
   the DP is O(units x line-candidates), and the result is cached per width
   like every layout.
3. The justified narrow column reads like set type: no line with gaps wider
   than a few times the space width when a hyphen or a different earlier
   break would avoid it.

## Involves

`alloy/src/rendertree/text/layout.rs` (a candidate-break model: a unit may
carry optional split points with the metrics of its halves; the optimal-fit
pass as an alternative to the greedy loop, reusing `open`/`cut`/`close`),
`shape.rs` (hyphenate + reshape halves through the word cache), the
`hyphenation` crate and its pattern data (size: en-US patterns are ~100 KB;
loading policy per language), `lang`/`hyphens` props in flux and types,
probe rows. Sequence: 1 first (it also improves greedy), then 2.

## Not here

Word-spacing caps or letter-spacing tracking to hide loose lines: no
Impeller support for tracking, and a slack cap only trades a loose line for
a ragged one. Column width is the app's lever meanwhile.
