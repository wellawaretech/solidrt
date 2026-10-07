---
title: Own glyph rasterizer behind the shaper seam
description: Text quality and shaping semantics are capped by Impeller's paragraph engine (grayscale AA only, no gamma or stem darkening, no glyph positions so carets re-shape every prefix, shaping cut at word boundaries, fallback not ours); the owned layout reduced the engine's job to shape-one-run and draw-one-run, so a second implementation with its own glyph atlas can replace it where quality matters.
created: 2026-08-17
---

# Own glyph rasterizer behind the shaper seam

Stage 1 started 2026-10-06 (the plan at the end of this file): the seam
made explicit, a second shaper and a glyph engine behind it, driven by
[2d-world-space-text](2d-world-space-text.md), whose world-space labels
are the first consumer. Stage 2 step 3, the same day, made the engine the
only shaper and the only drawer of `<text>`: Impeller's paragraph path is
gone.

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

## State (2026-10-06, evening)

| Part | State |
|---|---|
| 1. Fonts | built (`glyphs/fonts.rs`): harfrust + swash over the registered bytes, the wght axis per weight |
| 2. Shaping | built (`glyphs/shape.rs`): metrics match Impeller within a texel, carets in one pass |
| 3. Cells | both kinds built: masks in `glyphs/cells.rs`, MTSDF cells in `glyphs/msdf.rs` over the C shim (`alloy/csrc/msdf_shim.cpp`, bound by `glyphs/ffi.rs`) that `alloy/build.rs` compiles with the vendored msdfgen v1.13 core |
| 4. Atlas | built (`glyphs/atlas.rs`), the sub-rect upload in alloy and as `uploadTexture(id, data, rect)` |
| 5. Worker | built (`glyphs/worker.rs`), completion ends the hold and wakes the loop |
| 6. The seam | built, then collapsed in step 3: one shaper, `prepare_units` over the word cache, every `PreparedUnit` carries its glyphs |
| 8. Carets | built 2026-10-06: every caret stop comes off the engine's cluster map in one pass (`WordCache::carets`); the prefix re-shaping is gone |
| 7. `flux:font` | built, with types and docs; consumed by `@solidrt/2d`'s sprite font |
| 9. Draw half, step 1 | measured 2026-10-06 (Findings): a text layer per node (B) locks the TV at 50 where `<text>` presents at 25 today, a texture op per glyph (A) collapses to 9 |
| 10. Draw half, step 2 | built 2026-10-06 (Findings): the text atlas, the glyph pass and the text layer behind a switch; the TV's prose at 50 fps where Impeller gave 25 |
| 11. Draw half, step 3 | built 2026-10-06 (Findings): the switch flipped and removed, Impeller's paragraph path, typography context and metrics table deleted, `FontSet` the one reader of the font bytes; the HUD on the engine; fallback as policy; letter spacing and the width axis as shaper parameters and props; gradient text in the pass; text layers released when unbuilt; the glyph program compiled at raster start; `paraShapes`/`paragraphs` renamed `wordShapes`/`textLayers` |
| Tests | `alloy/src/tests/glyphs.rs` (18: the font set, carets, both cell kinds, blank glyphs, the packer and its eviction, the phases, the warm-up, fallback, the whitespace rule, letter spacing, the width axis), `text_baseline.rs` (the layout contract pinned over a corpus), `text_gradient.rs` (the layer's gradient mapping), `alloy/examples/text_layer.rs` (the pass against a CPU composite, the policy remaps, a gradient run), `packages/core/tests/text-warm.test.tsx`, the 2d text tests |

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
  promise. A camera open ends its hold from a polling task of its own, not
  a tick, so it does not deadlock.
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
- 2026-10-06, stage 2 step 1, the draw measurement
  (`probes/text-draw-bench.tsx`, read by `probes/text-draw-bench-read.ts`):
  a field of 40 lines of 16 px prose (1622 glyph quads) under a mover that
  demands a frame every refresh, the sustained rate read from the `/stats`
  window. "today" is `<text>` through Impeller (40 paragraph ops),
  "snapshot" each paragraph's `<text>` in a snapshot boundary (10 texture
  ops, direction B), "glyphs" one texture op per glyph from a mask atlas
  (1624 ops, direction A), "sprites" the same glyphs as one instanced draw
  of a 2d layer (the pass alone). The field replayed from a recording
  boundary or re-recorded every frame, and a scroll in place of the mover,
  changed none of the numbers.

  | device | today | snapshot (B) | glyphs (A) | sprites |
  |---|---|---|---|---|
  | TV (TPM171E, armv7, Mali-T860, 50 Hz, 2x) | 25 | 50 | 9 to 10 | 50 |
  | tablet (SM-T500, arm64, Adreno 610, 60 Hz) | 60 | 60 | 15 | 60 |
  | desktop (x86, Intel RPL-P, 60 Hz) | 60 | 60 | 60 | 60 |

  A collapses on both Android devices (some 300 missed presents per
  window on the TV). B locks the TV at its refresh where today's `<text>`
  already presents every other vsync: 40 paragraph ops are past the 20 ms
  raster budget on the armv7 (the UI thread idle at under 2 ms of paint),
  the display-list-op-cost finding on text. So B is not only the
  direction, it is the fix for prose on the TV. The glyph pass itself
  (sprites) costs nothing visible on any device. The tablet's rows are
  its first run; a second run presented at 30 Hz from its fourth row on
  (the panel's doing, p95 stayed at 1.2 ms), a ceiling and not a cost.

  The mask cells of printable ASCII (95 cells at 16 px, made on the
  worker, the frame that lands them included): desktop 7 to 9 ms, tablet
  36 to 49 ms, TV 96 to 99 ms, about 1 ms per cell on the armv7. With
  three subpixel phases a style's first paint would stall some 300 ms on
  the TV if its cells were made synchronously. The cell-making decision
  was refined on this number the same evening (see Decided): warm-up on
  the worker, a budgeted synchronous path, the worker past the budget,
  remaking as a miss.
- 2026-10-06, step 2 built the same evening. What exists:
  `AtlasPacker`/`GlyphAtlas` generic over the cell key with frame stamps
  and eviction by repack (`glyphs/atlas.rs`); `CellRequest` with the
  subpixel phase and a darkening input; the worker with two priorities;
  `TextAtlas` (`glyphs/text_atlas.rs`): one mask atlas per platform
  context keyed on face, device ppem, weight, style, phase and glyph,
  filled by warm-up jobs, the budgeted synchronous path at build and
  needed jobs past the budget, `WarmRequest`s resolved at frame start
  when the display scale is known; the glyph pass (`gl/glyphs.rs`,
  `RasterCmd::RasterizeGlyphs`, `Context::rasterize_glyphs`): one
  attributeless draw whose vertex stage derives the quad and corner from
  `gl_VertexID` and fetches the rects and color from a float texture the
  pass uploads, so no VAO or buffer, the four coverage modes as uniforms,
  premultiplied source-over into a single-sample adopted texture; the
  text layer (`text/mod.rs`): drawn by `Text::build` when the switch
  says engine, the raster of the text's painted box at the display scale
  times the ancestors' scale, retained on the `Text` itself and validated
  by fingerprint like its shaping cache (the prepared cache's generation,
  the shaping width, the scale, the texel size) rather than a paint-cache
  variant and composite surgery; the Impeller path shares the line-run
  extraction with it. `warmText(styles)` on `flux:rendertree` and
  `@solidrt/core`; `typeScaleStyles`/`warmTypeScale` in the components,
  `Window` warming on mount and on theme change.

  Verified: `alloy/examples/text_layer.rs` compares the pass against the
  same cells composited on the CPU pixel for pixel (orientation, atlas
  mapping, the premultiplied blend), a re-render in place, the polarity
  remap lifting light text's edges and leaving dark text alone; 655 alloy
  and 77 flux unit tests; the desktop with the switch on paints a
  prose screen as 12 texture draws and no paragraph ops at 0.2 ms of
  paint; the TV's "today" row of the probe reads 50 fps locked under the
  engine (25 under Impeller), its screen complete and crisp at 2x; the
  2d text tests and the new `packages/core/tests/text-warm.test.tsx`
  pass under both engines.

  Two findings that cost a round each. A text node inside a cached
  recording boundary is never rebuilt, so a layer drawn before its cells
  landed stayed incomplete forever: the tree now keeps the set of text
  nodes with incomplete layers and damages them at the frame cells land
  (`RenderTree::note_text_layer`, `composite::paint_phase`). And the
  stage 1 settle finding again: work in flight must be counted down on
  the worker thread when a job finishes, not at the frame that lands its
  cells, or the headless host's settle waits for a frame it will never
  step.
- 2026-10-07, the settle finding once more, completed. Step 2 counted the
  jobs in an atomic of the atlas's own that lattice's settle only read
  (`TextAtlas::pending`): settle awaits flux's holds, so while cells were
  on the worker it stepped a frame per look, and the 5000 ms app-time cap
  ran out in a fraction of a second of wall time. A release client made
  the warm-up first; CI's debug client did not (`text-warm.test.tsx`).
  Work in flight must be counted where settle waits: the atlas takes a
  `WorkHold` per job from a `HoldSource` the embedder sets per engine
  (`set_hold_source`; flux binds it to `PendingOps::in_flight("text
  cells")` in `gui::install`) and drops it on the worker thread, so the
  wait passes no app time and is bounded by the host's wall cap (30 s on
  the render host, the runner's per-test timeout under `sol test`). Two
  things found beside it: a job's done closure must latch the frame and
  wake the loop BEFORE it drops the hold, or whoever the drop releases can
  read no demand and report at rest with the cells unlanded (the stage 1
  `flux:font` and camera closures had the same order, reordered); and
  `warmText` queued its warm-ups for a frame nothing requested, which a
  bare `warmText(); settle()` would never run (`request_warm` latches the
  platform's frame request now, which the atlas is constructed with).

  Open after step 2, for step 3: the first engine frame of a screen costs
  the glyph program's compile and ten layer round trips (91 ms of paint
  on the desktop once; compile the program at context creation); a
  gradient text paint draws in its solid color through the layer; the
  synchronous budget makes about three cells a frame on the armv7, so a
  cold style there completes over a few frames through the worker; a
  long document's layers are held by their nodes until the text changes
  (no release of culled layers); `prepareText` over `flux:rendertree`
  carries `glyphs` while the engine draws (the types say so). No unit
  test covers the synchronous budget itself; the live client does.
- 2026-10-06, step 3, the gate. Over a corpus of 124 paragraphs (the
  gallery's strings, the newest changelog release, the layout probe's
  samples, numbers, paths, hashes, diacritics, Greek, Cyrillic and CJK) in
  seven styles over the three shipped Notos, Impeller and the engine
  agreed on 7196 units' advance, ink width, ascent and descent within a
  texel (the worst deviation 0.3 px, a descent in the mono style at line
  height 1.55) and broke every paragraph on the same units at 240, 400 and
  640 px. 133 units were left out as notdef on the engine: the CJK words
  (Impeller filled them from a system font, which the engine does not by
  decision) and a tab, which harfrust maps to glyph 0 where Impeller
  treated it as whitespace. The gate test went with the Impeller path; the
  corpus stays as `alloy/src/tests/data/text-corpus.txt` and the engine's
  own numbers over it as `text-baseline.txt` (breaks in every style, word
  metrics in one style per face, the vertical metrics once per style, 107
  KB), checked by `engine_matches_its_baseline` and regenerated by an
  ignored test after a deliberate change, so a harfrust or swash bump
  that moves a break fails a test and not an app.
- 2026-10-06, step 3, the deletion. What went: `TextDraw` and its
  environment variable, `ShaperKind`, `Shaper`, `Shaped`, the Impeller
  branch of `Text::build`, the paragraph style conversion, the caret
  scaling onto a drawn advance, `TypographyContext`, `register_font`,
  `FontMetricsTable` (a face reads its underline metrics at registration),
  the shaper argument of `prepare_units`. What the deletion found: four
  tree tests built a platform with no fonts and relied on Impeller's
  system-font fallback for their text to have a size; they register the
  Noto now (`tests::text_platform`). The stats HUD built an Impeller
  paragraph, so it draws through `Text::rasterize`, the text node
  rasterized outside a tree walk, and keeps its image to re-render into
  while the size holds; an image drawn before its cells landed refreshes
  the frame they do. `prepareText` on `flux:rendertree` reports no glyphs
  at all now: a `<text>`'s glyphs may come from any registered face, so
  only `flux:font`'s prepareText, whose ids name its own atlas's cells,
  reports them.
- 2026-10-06, step 3, fallback. A cluster the run's face has no glyph for
  (harfrust's glyph 0) is re-shaped on the next registered face whose
  character map covers its first character, faces registered under a role
  alias first, consecutive clusters that pick the same face as one piece;
  a glyph carries its face and the layer asks the atlas per (face, phase).
  Whitespace and control characters never reach that path: a cluster of
  them takes the space glyph at the space's advance (a tab is a space
  wide), which moved the baseline's tab line and one heading break and
  nothing else. `flux:font` shapes with `Fallback::None`: a handle is one
  face over one atlas. A test finds its character by shaping (the first
  code point the mono face shapes to notdef, that sans covers, and that
  stays a cluster of its own after a letter): a character-map difference
  is not enough, since harfrust composes some precomposed letters from a
  base and a mark, and a combining mark merges into the cluster before it.
- 2026-10-06, step 3, the two shaper parameters. `letterSpacing` (px) is
  added to the advance of the last glyph of every cluster, the last
  included, as CSS does; carets follow the pens. `fontStretch` (a CSS
  percentage, 100 normal) sets the `wdth` axis of a variable face, clamped
  to its range, on the harfrust instance, the swash metrics and the cell
  request; a face without the axis ignores it. The shipped Noto Sans has
  the axis (62.5 to 100), so condensed text works out of the box; the
  packaging question of instancing `wdth` out of the shipped fonts stays
  with the packaging item. Both are props on `<text>` and `<span>`, options
  of `measureText`, `prepareText`, `warmText` and `createFont`, parts of
  the run style, the shaping fingerprint and the atlas style key.
- 2026-10-06, step 3, gradient text. No app, example or component drew
  gradient text, and under Impeller only an absolute-units gradient ever
  did (a box-relative one was solid, since a paragraph has no bounds).
  The pass draws both now: a run's gradient resolves to the layer
  (`text/gradient.rs`: layer pixel to gradient parameter as two affine
  rows, the stops sampled into a 256-texel ramp), the layer's quads group
  by paint (the solid runs together, a group per gradient style), and one
  draw per group samples the ramp per fragment, tile modes included. A box
  gradient on text resolves against the text's own box: its wrap width by
  its lines' height, at the text origin. The example checks a red-to-blue
  run reads red at its first stem and blue at its last with the white
  word's exact coverage.
- 2026-10-06, step 3, layer release. A text's layer is stamped with the
  atlas frame it was last composited at; the paint walk's end releases the
  layers of texts unbuilt for 120 frames (the atlas's eviction age), so a
  long document holds the layers of what it shows. A text inside a reused
  recording boundary is never rebuilt, so its stamp ages while the
  recording's display list still holds the texture; dropping the handle
  frees nothing until the recording goes, and the text re-rasterizes once
  when the recording is next re-recorded, one pass. Accepted.
- 2026-10-06, step 3, verified: 663 alloy tests, 77 flux, the example's
  pixel asserts on the desktop GPU; flux and lattice compile; the sol
  tests and a device read are listed under "Where to pick up".

## Where to pick up (updated 2026-10-06, after stage 2 step 3)

Stage 1 and stage 2's steps 1 to 3 are built (the State table). Impeller
draws no text anywhere; the engine is the one shaper. Next is step 4,
coverage to color by eye: the four modes are uniforms already
(`CoveragePolicy`, settable through `PlatformContext::set_coverage_policy`),
so the step is the gallery toggle, the screenshots at 1x and 2x on both
polarities, the choice, then the per-DPI darkening (`LOW_DPI_DARKEN_EM` in
`text_atlas.rs` is zero until then) and retiring the Medium default and
the components' weight compensation (`policy.textWeightDelta`).

Left open by step 3, to settle before or with step 4:

- The eye and the device: the gallery, the console and the changelog shot
  at 1x here and at 2x on the TV were not looked at after the flip in the
  session that built it; the alloy example and the tests passed. Read
  the TV's text with the probe once more too.
- `packages/2d` applies its own `letterSpacing` over `flux:font` glyphs at
  layout time; the engine shapes letter spacing now, so the 2d layout could
  take it from the face instead (tiny.md).
- A fallback face is warmed with printable ASCII on first use like any
  style, which is wasted for a CJK face; harmless.

## Plan: stage 2, the draw half (prepared 2026-10-06, step 1 started the same day)

What it is: `<text>` drawn by the engine from mask cells, Impeller's
paragraph path deleted, and with it the quality and semantics this item
exists for (the symptom section): coverage to color under our policy,
stem darkening and hinting per DPI, shaping across word boundaries,
fallback as our policy. The decisions under "Decided" were taken
2026-10-06; the steps below are the combined list.

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

### Decided (2026-10-06)

- B, text nodes as rasterized layers, with step 1's numbers as the gate.
  The layer re-rasterizes while an ancestor's composite scale differs from
  the rasterized one: the pass is cheap, and a zooming card must not smear.
- Packaged fonts only once Impeller no longer draws text: the engine's
  `FontSet` resolves registered faces only, sans is the fallback, a role
  dropped with `false` has no face (the console drops serif today).
- The harness: identical breaks and advances within a texel, pixels
  reported not asserted; the Impeller baseline is captured before the
  paragraph path goes.
- One text atlas per engine, owned by the platform context, cells keyed on
  (face, size, weight, style, phase, glyph); `flux:font` keeps its
  per-handle atlases.
- Cell making, refined the same evening on the step 1 numbers (a mask
  cell costs about 1 ms on the armv7 TV, 0.1 ms on the desktop). The text
  atlas is a cache: a text layer retains pixels and never cell
  references, so an evicted cell, or one made at a stale display scale,
  is only ever a miss at the layer's next rasterization, and remaking is
  the same path as making. Three sources fill the one atlas. (1) Warm-up
  on the worker: printable ASCII at every phase for a style key the
  moment a build first sees it (its visible glyphs go first, through
  source 2), and a `warmText(styles)` call for the type scale at startup
  under the splash, which the components' typography system makes from
  its roles. (2) The synchronous path at build for a glyph missing now,
  in paint order, bounded by a per-frame time budget (a named constant of
  a few ms; time, not count, so it scales with the device), on a swash
  rasterizer of the UI thread's own. (3) Past the budget, the worker at
  "needed" priority ahead of warm jobs, the layer drawn without those
  glyphs and re-rasterized the frame they land. A single missing glyph
  never flashes on any device; a wall of unexpected glyphs never stalls a
  frame past the budget. The worker stays the only maker of msdf cells.
- Underlines stay display-list ops over the layer. Gradient text paint is
  checked across the apps before step 3 decides mask-under-gradient or a
  documented drop.

### Steps (combined)

1. **Measure.** `probes/text-draw-bench.tsx` and its reader
   `probes/text-draw-bench-read.ts`: today, snapshot (B), glyphs (A) and
   sprites (the pass alone, one instanced draw through a 2d layer), under
   a mover and a scroll, boundary on and off, on the TV and the desktop.
   The probe also logs the worker's time to make printable ASCII, the
   number behind the synchronous-rasterization decision. Done
   2026-10-06, the numbers under Findings: B locks the TV at 50 where
   `<text>` runs at 25 today, A collapses to 9.
2. **The atlas and the pass** (old steps 2 and 3). The shared text atlas
   with the composite cell key and three subpixel phases; mask cells with
   the policy inputs (hinting off, darkening by DPI); a raster command
   that draws a node's glyph quads from the atlas into an exact-size
   adopted texture, single-sample, the coverage-to-color policy a
   uniform; a text layer per node in the composite walk, composited
   through `BoundaryComposite` like a snapshot. Behind an engine-wide
   switch, Impeller still the default, so one build draws both.
   Verified by an alloy example with pixel asserts and the glyph tests.
   Built 2026-10-06 (Findings): `alloy/examples/text_layer.rs`,
   `SOLIDRT_TEXT_DRAW=engine`, `warmText` in `flux:rendertree` and
   `@solidrt/core`, the components' `Window` warming the type scale.
3. **The switch, fallback and deletion** (old steps 4, 6 and 7). The
   corpus gate compares breaks and advances on both shapers (pixels were
   not compared: Impeller's blending is the thing being replaced, not a
   reference); the switch flips and goes; fallback as policy lands in the
   same change (a notdef cluster re-shaped in the next registered face
   that covers it, roles first); then the paragraph path, the typography
   context, `register_font` and `FontMetricsTable` go, `FontSet` the one
   reader of the font bytes. Letter spacing and the width axis become
   shaper parameters and props; gradient text draws in the pass; the HUD
   draws through the engine. Built 2026-10-06 (Findings).
4. **Coverage to color, by eye** (old step 5). The spike's four modes
   behind a gallery toggle, screenshots at 1x and 2x on both polarities
   from 11 to 24 px, the user picks; then the per-DPI darkening policy,
   and the Medium default retires
   ([dpi-aware-default-font-weight](../backlog/dpi-aware-default-font-weight.md)
   closes).
5. **Platforms** (old step 8). The layer on Android, then Windows and
   macOS, the TV's frame cost re-read with the probe.

Old steps 2 and 3 fold together because the pass is unverifiable without
the atlas it samples; old 4, 6 and 7 fold because deleting Impeller's
path without fallback would turn every glyph it resolved through a system
font into a notdef box in the same change.

### Step 2 design (2026-10-06, before building)

**The atlas** (`alloy/src/rendertree/text/glyphs/`).

- `AtlasPacker` and `GlyphAtlas` become generic over the cell key
  (`K: Copy + Eq + Hash`), `Cell` carries its key. The `flux:font`
  handles keep `K = u16` (the glyph id): nothing changes for them or for
  `@solidrt/2d`.
- `TextAtlas` (new, `text_atlas.rs`): one per `PlatformContext`, a
  `GlyphAtlas<CellKey>` of mask cells. `CellKey { face: FaceId, ppem
  (device pixels per em, the f32's bits so equal sizes make equal keys),
  weight: u16, italic: bool, phase: u8, glyph: u16 }`.
- Every cell carries its etagere `AllocId` and a last-used frame stamp;
  a build `touch`es what it uses; when a growth would pass the cap the
  atlas frees every cell untouched for `EVICT_AFTER_FRAMES`, least
  recently used first, and repacks once. The shared packer gives the
  `flux:font` handles the same eviction, which closes
  [glyph-atlas-eviction](../backlog/glyph-atlas-eviction.md).
- The phase: `phase = floor(frac(x_device) * PHASES)` with `PHASES = 3`;
  the cell is rasterized at `offset(phase / PHASES, 0)` and its quad lands
  at `floor(x_device)`.
- `ensure(keys, deadline) -> misses`: the synchronous path within the
  frame's budget, the rest queued at `JobPriority::Needed`; `warm(style)`
  queues ASCII at every phase at `JobPriority::Warm`; `CellJob` gains the
  priority and the worker drains its channel into a two-level queue.
  Cells are made through the existing `Rasterizer` and `CellRequest`;
  synthetic darkening by DPI (the spike's strength, below 2x) rides on
  the request as a policy input.

**The pass** (`alloy/src/gl/glyphs.rs`, `alloy/src/raster/`).

- `RasterCmd::RasterizeGlyphs { quads, atlas, width, height, policy,
  into, reply }` with `GlyphQuad { dst: [f32; 4] (device px), src:
  [f32; 4] (texels), color: [f32; 4] }`; `into` re-renders retained
  storage at an exact dimension match, as the snapshot commands do.
- A `GlyphRig` on `RasterState`, compiled on first use like the copy
  program: a built-in program (vertex: the unit quad corner from
  `gl_VertexID`, the per-instance rects and color from a streaming
  instance VBO; fragment: coverage from the atlas's alpha, the policy
  applied, premultiplied output), one VAO, the instance VBO grown as
  needed. Draws into an adopted single-sample texture, cleared
  transparent, premultiplied source-over so kerned pairs and diacritics
  that overlap composite right. GLES 3.0, no MSAA.
- `CoveragePolicy { mode, gamma }` as uniforms: the spike's four modes
  (naive sRGB, linear light, polarity-aware remap, hybrid) from the
  start; step 4 picks the default and the control API sets it for the
  gallery toggle.

**The layer** (`rendertree/boundary.rs`, `composite.rs`, `text/mod.rs`).

- `PaintCache::TextLayer(TextLayerCache { texture, width, height, scale,
  valid, complete })`, built by `boundary::text_layer_node` for an
  `ElementKind::Text` when the engine draws text, on the same
  `painted_box` and `BoundaryComposite` path as a snapshot: the box is
  the text's `painted_extent` (ink slack included) in the node's frame,
  the raster at the display scale times the composite scale read off
  `ctx.to_window`, re-rasterized when that drifts past
  `LAYER_SCALE_TOLERANCE`, opacity and filters hoisted as for a snapshot.
- `Text::glyph_quads(platform, content, scale) -> (quads, misses)`
  replaces the draw loop of `Text::build` when the engine draws: lines
  and joined runs as today, each run shaped on `Shaper::Engine`, each
  glyph's key from (face, device ppem, weight, style, phase), the atlas
  `ensure`d within the budget; `complete` is "no misses". Decorations
  stay ops drawn after the layer's quad. Hit testing, carets and
  selection are untouched (they read layout, not pixels).
- Invalidation: `invalidate_paint` drops the cache as for a snapshot. An
  incomplete layer re-rasterizes on the frame the atlas reports landed
  cells (a flag the landing tick sets and the paint walk clears); a
  repack moves placements but a layer holds pixels, so only incomplete
  layers care.
- The switch: `SOLIDRT_TEXT_DRAW=impeller|engine`, read once into
  `PlatformContext`, default impeller through step 2 (step 3 flips it
  and removes it); the default shaper of `measureText` and `prepareText`
  follows it so layout and draw agree.
- `warmText(styles)` on `flux:rendertree` and `@solidrt/core`, called by
  the components' typography system for its roles at startup.

**Verification.** `alloy/examples/text_layer.rs`: a word through the
pass against the same cells composited on the CPU, pixel asserts per
policy mode; a layer re-rasterized at 2x; an incomplete layer completing
when its cells land. Tests in `alloy/src/tests/glyphs.rs`: the keyed
atlas, eviction and repack, the phases, the budget. A sol test for
`warmText`. The probe's "today" row re-read with the switch on, expected
to match the "snapshot" row.

Colour emoji is a stage of its own after this one: bitmap strikes (CBDT,
Noto Color Emoji) through swash into rgba cells, COLRv1 an open question
for swash, and ten megabytes of font a packaging decision. Bidi,
hyphenation, the rest of decoration and the own display list stay their
own items. The glyph-atlas eviction a terminal would need is
[glyph-atlas-eviction](../backlog/glyph-atlas-eviction.md).
