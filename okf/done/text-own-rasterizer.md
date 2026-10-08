---
title: Own glyph rasterizer behind the shaper seam
description: Text quality and shaping semantics are capped by Impeller's paragraph engine (grayscale AA only, no gamma or stem darkening, no glyph positions so carets re-shape every prefix, shaping cut at word boundaries, fallback not ours); the owned layout reduced the engine's job to shape-one-run and draw-one-run, so a second implementation with its own glyph atlas can replace it where quality matters.
created: 2026-08-17
completed: 2026-10-08
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
| 12. Coverage policy, step 4 | built 2026-10-08: `setTextRendering` on `flux:rendertree` and `@solidrt/core` (mode, gamma, contrast, stem darkening, hinting, live), DirectWrite's blend as a fifth mode and the default (gamma 1.8, contrast 1, no darkening), the Medium default and the components' weight compensation retired, `probes/text-coverage-probe.tsx`; picked by eye at 1x (headless render, DP-2) and 1.5x (tablet) |
| 13. Hinting, step 5 | built 2026-10-08 (Findings): swash replaced by skrifa and zeno in `glyphs/`, the light autohinter forced on below 2x (`Engine::Auto`, linear metrics preserved), hinters cached per thread, FreeType's embolden carried over for the synthetic bold; the layout baseline unchanged; the text layer snapped to the device grid (`grid_shift`), without which hinted rows blurred on composite; `hint` a mode (off, light, full = the mono target), on `setTextRendering` and `createFont` |
| 14. Platforms, step 5 (old 8) | read 2026-10-08 (Findings): the layer on Windows (ANGLE D3D11) and macOS (headless, ANGLE Metal) at HEAD, the TV re-read with `probes/text-prose-bench.tsx` (50 fps locked, no missed presents, the hinted switch no dearer than any cold style), the 2x TV and 1.5x tablet looks shot for the user (the tablet at 60 locked, a hinted cold style's first paint 27 ms) |
| Tests | `alloy/src/tests/glyphs.rs` (24: the font set, carets, both cell kinds, blank glyphs, the hinted cap height, the synthetic bold and slant, the packer and its eviction, the phases, the warm-up, fallback, the whitespace rule, letter spacing, the width axis, the rendering policy), `text_baseline.rs` (the layout contract pinned over a corpus), `text_gradient.rs` (the layer's gradient mapping), `alloy/examples/text_layer.rs` (the pass against a CPU composite, the policy remaps, a gradient run), `packages/core/tests/text-warm.test.tsx`, the 2d text tests |

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

Cut into [text-rasterizer-findings](../notes/text-rasterizer-findings.md)
when the plan closed, as `okf/README.md` asks: the measurements and the
traps that hold whether or not this plan existed.

## Where to pick up (updated 2026-10-08, after the platforms read)

Step 5 (platforms) is read on all four clients (the State table, the
Findings entry above): the layer paints and holds the refresh rate on the
TV, the tablet, Windows and macOS, the hinter costs nothing visible on the
armv7, the Mac is reachable headless. The 2x TV crop and the 1.5x tablet
crop are with the user for the look. With those judged, the plan is
complete: the engine is the one text path on every platform, with the
policy and the modes in place, and the plan moves to okf/done. What is
left of the text work lives in okf/tiny.md (the policy read-back, hinting
under a scale animation, the R8 mask atlas, synthetic bold under full
hinting).

The device recipe, for the next rendering change: `sol run <probe>
--project --port <N> --lan`, `sol android --port <N> --device <serial>`
(the player from `make -C lattice android-client ANDROID_ABI=<abi>` and
`lattice/android/gradlew assemblePlayerDebug -PsolAbi=<abi>`, `adb
install -r`; the TV is armeabi-v7a; an unlocked device, the player pauses
behind the keyguard), the builders' `win32-x64-msvc` exe launched on the
box with `--dev-server <ip>:<port>`, `sol render --project` on the Mac
builder's checkout; `/stats?window=5000&client=<id>` and a root crop
`/snapshot?client=<id>&node=1&x=&y=&width=&height=` per client.

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
