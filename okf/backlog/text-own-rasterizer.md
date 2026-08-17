---
title: Own glyph rasterizer behind the shaper seam
description: Text quality is capped by what Impeller's paragraph rasterizes (grayscale AA only, no gamma or stem darkening for light-on-dark, no letter spacing or variation axes in the C API); the owned layout reduced the engine's job to shape-one-run and draw-one-run, so a second implementation with its own glyph atlas can replace it where quality matters.
created: 2026-08-17
---

# Own glyph rasterizer behind the shaper seam

Kept as a shaped idea. Not scheduled; it may never be built. Written here
rather than in ideas.md because the seam and the reasoning are concrete.

## Symptom

Light type on dark backgrounds renders as hairlines that bleed at 1x
(Impeller antialiases text in grayscale only, no gamma-corrected blending or
stem darkening), which is why the default font weight is Medium instead of
Regular ([dpi-aware-default-font-weight](dpi-aware-default-font-weight.md)).
Letter spacing, variation axes ([font-stretch-axis](font-stretch-axis.md)),
LCD AA on desktop and a hinting policy per DPI are out of reach: not in
Impeller's C surface, and never going to be.

## The seam

Under the owned layout ([text-layout-owned](../done/text-layout-owned.md))
the engine's contract collapsed to two calls on one run:

- `shape(text, style) -> {advance, ink width, ascent, descent}` (today: a
  single-line Impeller paragraph, cached in the word cache)
- `draw(run, x, y)` (today: `draw_paragraph` of that object)

Segmentation, breaking, alignment, ellipsis, spans, atoms, floats,
selection and the app-facing primitives all sit above that and never see
the engine. Making the seam an explicit trait is the first, cheap step;
today it is the pair `WordCache::get_or_shape` + `draw_paragraph` in
`alloy/src/rendertree/text/`.

## Candidate second implementation

cosmic-text (rustybuzz shaping + swash rasterization, pure Rust), which also
brings variation axes and letter spacing. Caveat: Impeller has no
draw-glyphs primitive, so a non-Skia shaper brings its own rasterization: a
glyph atlas texture drawn with `DrawTextureRect` (or outlines as
`draw_path`), plus font discovery and fallback (system fonts, packaged Noto,
emoji), which today ride on the typography context. That is where the
quality lives: AA, gamma-corrected blending or stem darkening for
light-on-dark, subpixel positioning, optional LCD AA on desktop, hinting
policy per DPI. Evidence per-run shaping is not a fidelity problem: the
owned engine renders pixel-identical to Impeller's paragraph on Latin and
CJK samples.

## Done looks like

A second shaper selected per platform or per app, the Medium-weight
workaround retired where it runs, `Text.paragraph_engine` and the paragraph
path deleted (they exist as the reference until then).

## Involves

The trait; a glyph atlas (texture upload path exists: `flux:gpu` textures,
`DrawTextureRect`); font loading and fallback for cosmic-text from the same
`FontPayload`s; emoji (color glyphs) as its own problem. A large project.
