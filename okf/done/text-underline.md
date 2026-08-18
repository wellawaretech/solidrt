---
title: Underline for <text> and <span>
description: The owned text engine shapes one paragraph per wrap unit and Impeller's decoration skips trailing whitespace, so a delegated underline is gapped at every space; draw it ourselves per line from the fonts' own post-table metrics (ttf-parser), with CSS names and overrides, and mirror it on the Impeller paragraph path.
created: 2026-08-18
completed: 2026-08-18
---

# Underline for `<text>` and `<span>`

Done (2026-08-18). `textDecoration="underline"`, `textUnderlineOffset`,
`textDecorationThickness` on `<text>` and `<span>`; docs in `docs/core.md`,
measurements in [impeller-text-decoration](../notes/impeller-text-decoration.md).

## Why not delegate to Impeller

Impeller has `ImpellerParagraphStyleSetTextDecoration` (types mask, color,
style, thickness multiplier) and draws the font's own underline. But the
owned engine ([text-layout-owned](text-layout-owned.md)) shapes every wrap
unit `"word "` as its own single-line paragraph, and Skia's line painter
excludes trailing whitespace from decorations, so per-unit delegation yields
`word_ word_ word_`. Reshaping the units differently (leading space) touches
ink/advance and the breaker for a decoration; shaping each *line* as one
paragraph after layout would give continuity but moves draw positions off
the per-word measurements hit testing and carets use. Neither is worth it.

## What was done instead

- One rect per line in the rendertree (`alloy/src/rendertree/text/decoration.rs`),
  from the placed runs: first run start to last run ink end, so inner spaces
  are covered and trailing whitespace hangs, as Impeller does per line.
- Geometry from the font's `post` table, read with `ttf-parser`
  (default-features off, `std`) at font registration and kept in
  `PlatformContext::font_metrics()`, keyed by alias and family names. The
  rule is Skia's: a stroke of the font's thickness centered on
  `baseline + underlinePosition`, verified pixel-identical to Impeller for
  the shipped Notos. `fs/14` (Skia's no-metrics fallback) was rejected:
  43% too thick and 1 px high for Noto at 40 px.
- A family Impeller resolves through the system fallback has no bytes we
  can read; it gets Noto's ratios (0.10em / 0.05em). Relying on per-platform
  system fonts is the app author's choice; the overrides are the tool.
- CSS decorating-box semantics: the `<text>`'s underline is one line in its
  own metrics and color under everything (atoms excepted); a `<span>` with
  its own `textDecoration` adds its own line in its metrics and color. A
  first cut drew per run at each run's metrics; a small span inside big
  text then got a thinner, higher, gapped line.
- The Impeller paragraph path (`Text::paragraph_engine`) sets the same
  decoration per run so both engines stay aligned; there `textUnderlineOffset`
  has no equivalent and thickness becomes a multiplier of the font's, and
  Impeller decorates per run at that run's metrics.

Probes: `underline-probe.tsx` (repo root, both engines) and
`alloy/examples/underline_probe.rs` (Impeller's ghost-space rule and metrics).

## Left out

The rest of CSS text decoration (`line-through`, `overline`,
`textDecorationColor`, dashed/dotted/wavy/double) is
[text-decoration-rest](../backlog/text-decoration-rest.md). Skip-ink is a
deliberate non-goal: it needs glyph outlines, and Impeller does not do it
either; the line runs through descenders.
