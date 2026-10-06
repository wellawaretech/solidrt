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
| 3. Cells | both kinds built: masks in `glyphs/cells.rs`, MTSDF cells in `glyphs/msdf.rs` over the C shim (`alloy/csrc/msdf_shim.cpp`, bound by `glyphs/ffi.rs`) that `alloy/build.rs` compiles with the vendored msdfgen v1.13 core |
| 4. Atlas | built (`glyphs/atlas.rs`), the sub-rect upload in alloy and as `uploadTexture(id, data, rect)` |
| 5. Worker | built (`glyphs/worker.rs`), completion ends the hold and wakes the loop |
| 6. The seam | built (`Shaper`, `Shaped`, `ShaperKind`, `PreparedUnit.glyphs`); `prepareText`'s default stays Impeller |
| 8. Carets | built 2026-10-06: every caret stop comes off the engine's cluster map, on either shaper; an Impeller-shaped word's stops are scaled onto its drawn advance; the prefix re-shaping is gone (`WordCache::carets`) |
| 7. `flux:font` | built, with types and docs; consumed by `@solidrt/2d`'s sprite font |
| Tests | `alloy/src/tests/glyphs.rs` (10: the seam, carets on both shapers and their cost, both cell kinds, blank glyphs, the packer), `packages/core/tests/gpu-upload.test.tsx`, the 2d text tests (an msdf run at twice the face size with its outline among them) |

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
   The mask kind was built first (that session could not read the
   vendored headers); the MTSDF kind followed the same day behind the
   same `Cell` type.
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
- 2026-10-06, the MTSDF cells: msdfgen's inside is the right-hand side
  of travel, so an outline wound clockwise with y up (TrueType) is
  positive inside and a counter-clockwise one (CFF) is inverted; the
  engine tells the font's sense from its largest contour's signed area
  and reverses the whole shape when it is counter-clockwise. The outline
  stays y up all the way: flipping y would mirror the winding and invert
  the field. The rows come out top first by declaring the output bitmap
  `Y_DOWNWARD` (v1.13's `BitmapSection` orientation; the generator
  reorders the rows itself). The shim keeps msdfgen's own importer rules
  for degenerate commands (a line or quadratic ending on the pen is
  dropped, a cubic too unless its controls span area) and closes every
  contour back to its start, which `Shape::validate` requires.
- etagere refuses an empty allocation, so a blank glyph (a space) is
  placed at a zero-sized spot without one, for either cell kind; before
  that a mask space took a 2x2 padding ring and an msdf space would have
  grown the atlas to its cap and come out missing.
- 2026-10-06, carets: an Impeller-drawn word's caret stops now come from
  the engine's shaping of the same word, which costs one cached shape per
  word instead of one per grapheme prefix (`Hamburg` put two entries in
  the word cache where it put eight). The engine's positions sit within a
  texel of Impeller's (the seam finding), and scaling them onto Impeller's
  advance makes a word's end stop exactly the next word's pen, so the
  residual is inside words only. The one divergence: a role an app drops
  with `false` draws from the system font through Impeller, a face the
  engine never sees, and its carets come from the engine's fallback face.
  That path ends with stage 2 either way.

## Where to pick up (updated 2026-10-06, after the MTSDF kind landed)

Steps 1-5 of the earlier list are done, the same day, once the vendored
headers could be read: `alloy/build.rs` compiles the msdfgen core and
`alloy/csrc/msdf_shim.cpp` through cc (`[build-dependencies] cc`,
forge's pin); `glyphs/ffi.rs` binds the shim and `glyphs/msdf.rs` turns a
swash outline into an MTSDF cell, wired as `CellKind::Msdf` in
`cells.rs`; `alloy/src/tests/glyphs.rs` pins the H field and blank
glyphs, `packages/2d/tests/text.test.tsx` an msdf run at twice the face
size with its outline; the sprite font and `flux:font` default to "msdf"
cells.

The carets followed the same day (part 8 of the State table): every
consumer of `prepareText` carets, TextInput first, reads the engine's
stops now, and nothing re-shapes prefixes. Stage 1 is complete; what is
left of this item is stage 2 below.

## Plan: stage 2, the draw half (prepared 2026-10-06, not started)

What it is: `<text>` drawn by the engine from mask cells, Impeller's
paragraph path deleted, and with it the quality and semantics this item
exists for (the symptom section): coverage to color under our policy,
stem darkening and hinting per DPI, shaping across word boundaries,
fallback as our policy. The steps are a proposal; the decisions under
"Decide first" are the user's before step 1 starts.

### The question that shapes everything: how glyph quads reach the screen

Today a line's run of same-styled words is ONE `draw_paragraph` op, and
Impeller batches the run's glyphs from its own atlas. Three ways to draw
ours:

- **A. A display-list op per glyph**: `draw_texture_rect` from the atlas
  texture adopted into Impeller (the `<texture>` bridge). A loop in
  `Text::build`, nothing else. But the TV ceiling is per-op CPU cost
  ([display-list-op-cost](../backlog/display-list-op-cost.md): 800 ops
  run at 27 fps where 50 run locked), and a screen of prose is thousands
  of glyphs. Fine for labels, not for a document.
- **B. Text as rasterized layers**: a GL glyph pass on the raster thread
  draws a text node's visible glyph quads from the atlas into a texture of
  the node's painted box, composited as one `draw_texture_rect`: the
  repaint boundary's rasterization path (`boundary.rs`) with a glyph
  pipeline in place of Impeller's replay. One op per text node (today one
  per line run), the glyph pipeline being the 2d sprite layer's on the
  Rust side (instanced quads over the atlas, mask or field decode; alloy's
  gpu module runs such pipelines and draw targets for `flux:gpu` already).
  Memory is the visible text's pixels at display scale; offscreen nodes
  drop their texture through the texture cache tier. Coverage to color is
  that pass's fragment shader, so the policy is a uniform.
- **C. Wait for the own renderer** ([replacing-impeller](../notes/replacing-impeller.md)),
  where text lands last. B is the text engine that renderer needs anyway:
  its glyph pass moves from a layer to a direct draw, nothing is thrown
  away.

Recommendation: B, gated by the measurement in step 1.

### Steps

1. **Measure before building.** A probe with a prose screen (some 2000
   glyphs over 40 lines) drawn three ways, on the TV and the desktop:
   `draw_paragraph` as today, per-glyph texture rects (A, a throwaway
   loop), and one layer per text node (B, mocked by a snapshot
   `repaintBoundary` view around each text, which already rasterizes it
   to a texture drawn as one op). Sustained frames per second by the
   saturation method and the
   op count. Expected: A collapses on the TV, B matches or beats today.
   Numbers decide; this plan assumes B from here.
2. **Mask cells for drawing.** `CellKind::Mask` gains the subpixel phase
   (a third of a pixel, three per glyph, the spike's choice) and the
   policy inputs (hinting off, synthetic darkening by DPI). Cells for
   `<text>` key on (face, size, weight, style, phase): one atlas per face
   with the key on the cell, or one per key as `flux:font` makes today,
   is a design point to settle here; `flux:font`'s handle stays the
   app-facing API over it. Eviction
   ([glyph-atlas-eviction](../backlog/glyph-atlas-eviction.md)) moves into
   this stage: prose at several sizes turns over more cells than labels.
3. **The glyph pass.** In alloy: a render pipeline (unit quad instanced
   over the atlas; vertex from pen position and cell placement; fragment
   coverage to color under the policy uniform) on the raster thread, and a
   text layer per node, `boundary.rs`'s rasterization with the pass as its
   content. Color per glyph run from the run's paint; opacity and filters
   through the existing boundary composite; decoration (underline, ours
   already) drawn into the same layer.
4. **The draw.** `Text::build` emits the layer; `draw_paragraph`,
   `Shaped::Paragraph` and `ShaperKind::Impeller` go, the word cache holds
   `ShapedGlyphs` only, `prepareText` and `measureText` shape on the engine.
   Joined-run drawing stays (one shape per line run). The gate before the
   Impeller path is deleted: a pixel harness over the components gallery
   and the changelog text at 1x and 2x against an Impeller baseline (the
   method of [text-layout-owned](../done/text-layout-owned.md)), asserting
   identical line breaks and advances within a texel; the pixels differ by
   design, so the harness reports them for the judgement in step 5 rather
   than failing on them.
5. **Coverage to color, by eye.** The spike's four modes (naive sRGB,
   linear light, polarity-aware contrast remap, hybrid) behind a debug
   toggle on the gallery; screenshots at 1x and 2x, light on dark and dark
   on light, 11 to 24 px; the user picks. Then the per-DPI policy (stem
   darkening below 2x, none above) and the Medium default weight retires
   ([dpi-aware-default-font-weight](../backlog/dpi-aware-default-font-weight.md)
   closes).
6. **Fallback as policy.** A cluster shaped to notdef is re-shaped in the
   next registered face that covers it, in registration order with the
   role aliases first. Packaged fonts only, no system font discovery: one
   known app, the same pixels on every platform. A role dropped with
   `false` then has no face at all, which the fonts doc says.
7. **Cleanup.** Three font parsers to one: `FontMetricsTable` (the post
   table) and Impeller's typography context fold into `FontSet`;
   `register_font` on Impeller goes; the `impellers` typography imports
   leave the rendertree. The letter-spacing and width-axis items
   ([font-stretch-axis](../backlog/font-stretch-axis.md)) become plain
   shaper parameters.
8. **Platforms.** The pass is plain GLES 3.0; verify the layer texture's
   adoption and the TV's frame cost on Android, then Windows and macOS.

Colour emoji is a stage of its own after this one: bitmap strikes (CBDT,
Noto Color Emoji) through swash into rgba cells, COLRv1 an open question
for swash, and ten megabytes of font a packaging decision. Bidi,
hyphenation, the rest of decoration and the own display list stay their
own items.

### Decide first

- B (text as rasterized layers) as the direction, with step 1's numbers
  as the gate.
- Packaged fonts only once Impeller no longer draws text (no system font
  discovery, the `false` role drop becomes "no face").
- The harness's contract: identical breaks and advances within a texel,
  pixels reported not asserted.

Stage 2 (the draw half) stays its own plan, as written above. The
glyph-atlas eviction a terminal would need is
[glyph-atlas-eviction](../backlog/glyph-atlas-eviction.md).
