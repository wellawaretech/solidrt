---
title: Own glyph rasterizer behind the shaper seam
description: Text quality and shaping semantics are capped by Impeller's paragraph engine (grayscale AA only, no gamma or stem darkening, no glyph positions so carets re-shape every prefix, shaping cut at word boundaries, fallback not ours); the owned layout reduced the engine's job to shape-one-run and draw-one-run, so a second implementation with its own glyph atlas can replace it where quality matters.
created: 2026-08-17
---

# Own glyph rasterizer behind the shaper seam

Stage 1 started 2026-10-06 (the plan at the end of this file): the seam
made explicit, a second shaper and a glyph engine behind it, driven by
[2d-world-space-text](2d-world-space-text.md), whose world-space labels
are the first consumer. Impeller keeps drawing `<text>` until a later
stage replaces the draw half.

## Symptom

Light type on dark backgrounds renders as hairlines that bleed at 1x
(Impeller antialiases text in grayscale only, no gamma-corrected blending or
stem darkening), which is why the default font weight is Medium instead of
Regular ([dpi-aware-default-font-weight](dpi-aware-default-font-weight.md)).
Letter spacing, variation axes ([font-stretch-axis](font-stretch-axis.md)),
LCD AA on desktop and a hinting policy per DPI are out of reach: not in
Impeller's C surface, and never going to be.

The shaping semantics are capped the same way (from the 2026-08 project
review):

- Each wrap unit is its own single-line paragraph, so shaping context ends
  at every word boundary: no cross-word kerning or ligatures, and font
  fallback is whatever Impeller picks per word, with no policy of ours.
- Impeller exposes no glyph positions (its glyph-info bounds come back
  without them), so caret stops shape every grapheme prefix of a word
  (`WordCache::caret_stops`): O(n^2) shaping per word, kerning lost at the
  cut, and each prefix becomes a full entry in the word cache. Editing-only
  and cached, so cheap for prose, but a long unbroken token (URL, hash)
  pays a multi-ms first caret placement and pollutes the cache.

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
workaround retired where it runs. (`Text.paragraph_engine` and the paragraph
path were kept as the reference implementation until 2026-09-02, then
deleted: the owned engine had been the only path in use, verified
pixel-identical.) A real shaper also
returns per-cluster advances, so caret stops become one O(n) read of the
shaped run (prefix re-shaping deleted), runs can shape across word
boundaries, and fallback (a notdef cluster re-shaped in a covering font)
becomes our policy - mechanism free, policy still to build.

## Involves

The trait; a glyph atlas (texture upload path exists: `flux:gpu` textures,
`DrawTextureRect`); font loading and fallback for cosmic-text from the same
`FontPayload`s; emoji (color glyphs) as its own problem. A large project.

## State (2026-10-06)

| Part | State |
|---|---|
| 1. Fonts | built (`glyphs/fonts.rs`): harfrust + swash over the registered bytes, the wght axis per weight |
| 2. Shaping | built (`glyphs/shape.rs`): metrics match Impeller within a texel, carets in one pass |
| 3. Cells | mask kind built (`glyphs/cells.rs`); the MTSDF kind waits for the msdfgen shim (reading the vendored headers was refused by the session's code-integration guard; the submodule is in place at v1.13) |
| 4. Atlas | built (`glyphs/atlas.rs`), the sub-rect upload in alloy and as `uploadTexture(id, data, rect)` |
| 5. Worker | built (`glyphs/worker.rs`), completion ends the hold and wakes the loop |
| 6. The seam | built (`Shaper`, `Shaped`, `ShaperKind`, `PreparedUnit.glyphs`); `prepareText`'s default stays Impeller |
| 7. `flux:font` | built, with types and docs; consumed by `@solidrt/2d`'s sprite font |
| Tests | `alloy/src/tests/glyphs.rs` (7), `packages/core/tests/gpu-upload.test.tsx`, the 2d text tests |

## Plan: stage 1, the glyph engine behind the seam (started 2026-10-06)

Decided with the world-space text item: the glyph producer that item
needs is this item's first stage, built once in alloy and reused when
the draw half follows. Impeller is not replaced here; `<text>` keeps
shaping and drawing through it, pixel-identical to today.

What stage 1 builds, in `alloy/src/rendertree/text/glyphs/` (engine-
independent: no `impellers` import anywhere under it):

1. **Fonts.** The `FontPayload`s `build_typography` registers get a third
   consumer: per payload a harfrust face (shaping) and a swash font
   (outlines, masks), looked up by alias and family name exactly as
   `FontMetricsTable` resolves them. The shipped Notos are variable, so
   `fontWeight` is the wght axis; a static font emboldens synthetically
   at 600 and up (the spike's stem darkening).
2. **Shaping.** `shape_word(font, text, style) -> ShapedGlyphs`: glyph
   ids, kerned positions and a cluster map from harfrust, metrics
   (`RunMetrics`) from the face's hhea/OS2 scaled to the size, line
   height as Impeller applies it. Caret stops read off the cluster map
   in O(n). harfrust over rustybuzz: rustybuzz was archived in July 2026
   in its favour.
3. **Cells.** Two kinds, one allocator: `Msdf` (an MTSDF cell at a fixed
   size per em and range, from swash outlines through msdfgen 1.13,
   vendored at `alloy/vendor/msdfgen` and reached through a thin C shim
   compiled by `alloy/build.rs`), and `Mask` (an exact-size coverage
   mask at a subpixel phase, what the draw half and a terminal will use).
   The mask kind is built first: reading the vendored msdfgen headers
   was refused by the session's code-integration guard, so the shim
   waits for that permission; the atlas, worker and plugin are built and
   tested against masks, and the MTSDF kind slots in behind the same
   `Cell` type.
4. **Atlas.** An etagere shelf allocator over a mutable rgba8 texture the
   engine registers in `alloy::Context`, grown in place through the
   id-stable resize, cells written by a new sub-rect upload
   (`UpdateTextureRects`, one raster command per flush with one mip
   regeneration), the same primitive `2d-atlas-runtime-packing` waits
   for, so it is exposed to JS as `uploadTexture(id, data, rect)` in the
   same change.
5. **Worker.** Cell generation on one background thread per engine
   (request glyph ids, receive cells), applied to the atlas at the next
   flush, so a glyph the app has not used before never stalls a frame.
6. **The seam.** `ShapedWord` carries either an Impeller paragraph or a
   `ShapedGlyphs`; the word cache keys on the shaper; `prepare_units`
   takes the shaper. `measureText`/`prepareText` over a font handle run
   on the engine shaper and each `TextUnit` then carries its glyphs.
7. **`flux:font`** (gui): create a font atlas for a registered family at
   a cell kind, size and range; prepare text over it; request glyph cells
   and await their arrival (a work-in-flight hold, so `settle` waits);
   read the atlas record and the font metrics; destroy. Types in
   `packages/flux-types/gui/font.d.ts`, `"font"` in `Flux.capabilities`.

Pins: harfrust =0.14.0, swash =0.2.10, etagere =0.3.0, cc =1.5.1 (build),
msdfgen v1.13 (MIT). Rejected: the `msdfgen` bindings crate (last release
2023) and fdsm (a single-publisher rewrite).

Tests, in `alloy/src/tests/`: `glyphs.rs` (a word shaped by the engine and
by Impeller agree on advance within a texel over the shipped Notos; caret
stops match the cluster map; the allocator grows and keeps placements; a
mask cell's coverage sums to the glyph's ink), `texture.rs` (a sub-rect
upload lands where asked). The seam test is what gates switching
`prepareText`'s default shaper later; it is not switched in stage 1.

Stage 2, later and its own plan: the draw half (`draw(run)` from the mask
atlas, Impeller's paragraph path deleted), fallback as our policy, colour
emoji, the hinting and gamma policy per DPI the spike explored.

## Findings

- 2026-10-06, the seam check: on the shipped Noto Sans at 16 px, regular
  and bold (the wght axis), a word shaped by the engine (harfrust) and by
  Impeller agrees on advance, ascent and descent within a texel, and the
  engine's caret stops (read off the cluster map in one pass) land within a
  texel of Impeller's prefix re-shapes (`alloy/src/tests/glyphs.rs`). The
  vertical metrics that match Impeller: hhea ascent and descent scaled to
  the size, the leading folded into the descent, the whole box re-scaled to
  `size * lineHeight` when a multiplier is set (Skia's height override).
- harfrust shapes in font units (no size); one scale factor to pixels per
  word, so a shaped word is size-independent up to that multiply.
- ttf-parser 0.25 without its `variable-fonts` feature has no axis
  enumeration; the engine reads a face's axes through swash instead, which
  it parses anyway.
- A glyph request's hold must end on the WORKER, not at the tick that
  lands its cells: the headless host's `settle` awaits "nothing in
  flight" before it steps a frame, so a hold released by a frame hook
  never ends (observed: `In flight: 1 glyph cells, frame 0`). The shape
  that works everywhere: the hold travels with the job and drops when the
  cells are made, the same closure latches the platform's frame request
  and calls the main loop's wake (`Context::frame_wake`, what a YUV sink
  uses), and the frame that follows lands the cells and settles the
  promise. A camera open settles from its tick too and would deadlock the
  same way under settle; not changed here.
