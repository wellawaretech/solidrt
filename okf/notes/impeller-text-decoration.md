---
title: Impeller text decoration, measured
description: What Impeller's paragraph underline does and does not do - trailing whitespace is never underlined, the stroke is the font's post thickness centered on baseline + underlinePosition, and the C API exposes no underline metrics - with the shipped Notos' numbers.
created: 2026-08-18
---

# Impeller text decoration, measured

Cut from [text-underline](../done/text-underline.md). Probe:
`alloy/examples/underline_probe.rs` (offscreen render, red decoration,
pixel scan). impellers 0.4.2, engine SHA in the crate's `ENGINE_SHA`.

## Whitespace (single-line paragraphs, 40 px, default family)

```
"Hello"         ink  98.48 advance  98.48  underline 98 px
"Hello "        ink  98.48 advance 109.73  underline 98 px   trailing space skipped
"Hello   "      ink  98.48 advance 132.23  underline 98 px
" Hello"        ink 109.73 advance 109.73  underline 109 px  leading space underlined
"Hello world "  ink 216.33 advance 227.58  underline 216 px  inner space underlined
```

Skia's line painter excludes trailing ("ghost") whitespace from decorations,
as browsers do at a line end. Anything that shapes per word therefore gets a
gap at every space; per line it is continuous.

## Geometry

Skia draws a decoration as a stroked line of the font's `post`
`underlineThickness`, centered on `baseline + underlinePosition` (the
OpenType value is nominally the top of the stroke; Skia centers on it).
Noto Sans / Serif / Sans Mono, all `upem 1000, underlinePosition -100,
underlineThickness 50`, at 40 px with baseline 42.28: rows 45.25..47.25,
i.e. center 4.0 px below the baseline, 2.0 px thick. Ratios: 0.10em / 0.05em.
Skia's no-metrics fallback `fontSize / 14` (2.86 px both) does not match.

## API

`ImpellerTextDecoration { types mask (underline | overline | line-through),
color, style (solid | double | dotted | dashed | wavy), thickness_multiplier }`
on a paragraph style; per pushed style, so per run. No absolute offset. The
C API exposes no underline or strikeout metrics (LineMetrics has ascent,
descent, baseline, height, width, left only). No skip-ink: the line runs
straight through descenders.
