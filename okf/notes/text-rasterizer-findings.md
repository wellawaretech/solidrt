---
title: What the glyph engine and the text layer found
description: Facts the own-rasterizer work surfaced that hold without it - harfrust against Impeller's shaper, the per-device cost of a paragraph op versus a text layer, mask cell times on the armv7, the coverage-to-color modes and DirectWrite's blend, skrifa and zeno traps (inspect before render, embolden over stroke, the light target and linear metrics, the mono target for stems), the layer off the grid, the hinter's cost on the TV, and the Mac rendering headless over ssh.
created: 2026-10-08
---

# What the glyph engine and the text layer found

Cut from [text-own-rasterizer](../done/text-own-rasterizer.md) when it
closed, as `okf/README.md` asks: each of these is true whether or not that
plan existed. The entries keep their dates and their step numbers, which
refer to that plan.


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
- 2026-10-06, stage 2 step 1, the draw measurement (`probes/text-draw-bench.tsx`
  and its reader, retired 2026-10-08 once the decision was taken: this
  table is their reading):
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

- 2026-10-08, step 4, the instrument. The policy became a live setting of
  the platform context (`TextRendering`: the coverage policy plus an
  optional stem darkening override in em per side; the darkening is part
  of the cell's style key, so a change makes new cells and the old age
  out), counted by a generation every text layer records and a dirty flag
  the frame start turns into paint damage on every text with a layer, the
  same path cells landing take, so a text inside a reused recording
  boundary redraws too. DirectWrite's grayscale blend went in as
  `CoverageMode::DirectWrite` from Windows Terminal's published shader and
  gamma-ratio table (MIT): an enhanced-contrast boost faded out by the
  text's lightness, then an alpha correction keyed on its intensity. What
  the example found about it (`text_layer.rs`): at gamma 1.8 it lifts
  white text's edge coverage and thins black text's mid coverage, the
  gamma-aware counterpart of naive sRGB blending, and black's curve is not
  monotone (faint coverage rises with the boost, mid coverage falls under
  the correction), so the example asserts counts and solid texels, not a
  direction per texel. Verified: the knob changes a line's pixels on the
  live client and restores them exactly on the way back (snapshot
  hashes), 28 glyph and text tests, the example's asserts on the desktop
  GPU. `sol render` found a `--client` is needed beside a running dev
  client (both default to client0's storage).

- 2026-10-08, step 4, the pick. On the 1x sheet (headless render, so exact
  1x pixels) and live on a 1x monitor, DirectWrite's blend read best at
  its Windows defaults (gamma 1.8, contrast 1); the three remaps and the
  darkened rows (0.015 and 0.03 em) were not needed once it lifted light
  text's edges. It is the default now, the Medium default weight is
  Regular, and the components' `textWeightDelta`, `typeWeight`,
  `lightOnDark` and the polarity argument of `typeStyle` are gone (the
  backlog item dpi-aware-default-font-weight closed). The 2x read is still
  owed: the TV was unreachable, the tablet (1.5x) agreed with 1x.
- 2026-10-08, step 4, the blur at 12 px and what hinting can do. Unhinted
  outlines with only the baseline snapped put the x-height and cap height
  wherever the size lands them: Noto Sans's 0.536 and 0.714 em sit near a
  row at 11 and 13 px (5.90 and 7.85, 6.97 and 9.28) and mid-row at 12 px
  (6.43 and 8.57), so 12 px reads blurry beside 11. swash's `hint(true)`
  changed nothing because the shipped Notos are the variable builds with
  no instructions (no `fpgm`, no `cvt`, a 7-byte `prep`, zero glyphs
  instructed) and swash drives skrifa's hinter with its default engine,
  AutoFallback, which copies FreeType's rule and sends a TrueType font
  with a non-empty `prep` to the interpreter. skrifa's autohinter (the
  fontations port of FreeType's, what Chrome ships) forced on with
  `Engine::Auto` in `SmoothMode::Light` does the right thing, measured on
  the shipped Noto Sans in a scratch crate: at 12 px x 6.44 -> 7.00, H
  8.56 -> 9.00, o -0.12..6.55 -> 0.00..7.00; at 14 px x 7.50 -> 8.00; at
  16 px H 11.42 -> 12.00; x extents untouched in every case, so the
  subpixel phases keep working. swash hard-codes its hinting mode and
  hides the engine choice, so the clean route is skrifa directly.

- 2026-10-08, step 5, the port. swash went: `glyphs/fonts.rs` reads the
  axes, the family names and the underline metrics through skrifa
  (`MetadataProvider`), `shape.rs` the vertical metrics (skrifa's descent
  is the table's negative descender, negated back), `cells.rs` draws
  each glyph into `outline.rs` (an `OutlinePen` over zeno path commands)
  and renders the mask with zeno at the subpixel phase, bottom-left
  origin, the same calls swash made inside, with one trap: zeno reports
  the placement's `top` relative to the baseline only once `inspect` has
  sized the mask, so a `render()` without it puts every cell at top 0.
  The synthetic bold and the darkening are FreeType's outline embolden
  carried over from swash's port (the plan's stroke-plus-fill would
  over-count every edge texel the stroke straddles, and zeno cannot
  stroke one side), so bold cells are unchanged. The hinter: a
  `HintingInstance` per (font, ppem, normalized coordinates) cached in
  each thread's `Rasterizer`, keyed on the font's allocation and pinning
  it, with `Engine::Auto` over the font's precomputed `GlyphStyles` and
  the light smooth target preserving linear metrics; a hinter skrifa
  refuses draws unhinted. `LOW_DPI_HINT` is on. Verified: the layout
  baseline passes untouched (advances, ascents, descents and breaks
  identical to swash's), 670 alloy tests including a new one that reads
  the H's top row at 12 px (unhinted the cap height crosses it at 0.57
  of the row below, hinted the two rows are equal, the box's width and
  left unchanged) and one for the embolden and the slant, the text layer
  example's pixel asserts on the desktop GPU; the lock drops swash and
  yazi, nothing else moves. The 1x sheet (the probe rendered headless,
  hinted over unhinted, 11 to 16 px) was sent for the user's read; on it
  12 and 14 px sit on their rows like 11 and 13, the remaining softness
  is the stems, which the light mode leaves to the subpixel phases.
- 2026-10-08, step 5, the layer off the grid. Live on the 1x monitor the
  12 px line read worse than 11; the row ink read off the tree (a node
  snapshot in raw format, alpha summed per row) said why: the 11 px
  line's x-height and baseline each sat in one row (17 to 74, 78 to 6),
  the 12, 13 and 14 px lines' were split over two (64/75, 69/57 at 12 px),
  and the headless frame agreed. The cells were right; the layer was
  composited at the node's logical position (y 79.67), so Impeller
  resampled every row a fraction off and undid the snapping the quads do
  inside the layer. Fixed in `build_layer`: `grid_shift` maps the layer's
  origin through `ctx.to_window` to device pixels and shifts the quad by
  the residual, under a plain translate-and-scale only (a rotation has
  no grid), so the layer lands on whole device pixels at every scroll
  offset without re-rasterizing, and text moves by under a pixel as in
  every browser. A text inside a snapshot boundary still inherits the
  boundary's fractional position, as does every border and icon: the
  general paint-time snap is okf/backlog/pixel-snapped-paint-boxes.md,
  which retires `grid_shift` when it lands. What remains soft on
  purpose is the stems: the light mode leaves x alone for the subpixel
  phases, Chrome's look on Linux. (Landed the same day,
  okf/done/pixel-snapped-paint-boxes.md: the walk snaps every box, the
  layer snaps its quad with the shared `grid::shift` against the grid
  map, and a text inside a snapshot boundary is on the grid too.)
- 2026-10-08, step 5, the full mode. The user asked whether every font
  should read crisp; the answer is the mode question, so `hint` became a
  mode: `Hint::{Off, Light, Full}` on the cell request, the style key and
  `TextRendering` (`setTextRendering({ hint })` takes `false`, `true` for
  light, `"light"`, `"full"` or `null`), and `flux:font`'s `createFont`
  takes the same `hint` option per mask atlas, off by default, reported
  by `fontAtlas`, since a terminal grid is where the full mode lives: its
  cells sit at whole pixels anyway. A full-hinted `<text>` cell is made at
  one phase and its quad placed at the nearest pixel. Two things the port
  of FreeType's autohinter taught: `preserve_linear_metrics` makes the
  autohinter treat any target as light (horizontal hinting off), so the
  full target cannot keep linear metrics (layout reads the shaper's
  advances anyway); and the grayscale "normal" target only nearly aligns
  stems (a 1.1 px stem fills one column at 244 and spills 36 into the
  next), so the full mode is the mono target rasterized with
  anti-aliasing: stem widths and positions rounded to whole pixels,
  curves still smooth. The test reads the H's top row at 12 px: light has
  no solid column, full has exactly two and nothing between. Judged live
  on the 1x monitor: stems solid, the text thinner (a 1.1 px stem is 1 px),
  and at 11 px Regular and Medium become the same glyphs, since 1.1 and
  1.25 px stems both round to 1 px and only a stem past 1.5 px gets 2;
  with the walking text, the reason full is a per-app and per-atlas
  choice and not the default. Why nobody runs full any more is recorded
  in the Where to pick up section.
- 2026-10-08, step 5, the platforms. `probes/text-draw-bench.tsx` was
  retired, so `probes/text-prose-bench.tsx` took its place: the same 40
  lines of 16 px prose under a mover demanding every refresh, plus a
  second field at 17 px Medium that nothing warms (`c`, or POST
  /debug?name=cold) so the frame that first paints it pays the
  synchronous cell path, and `h` / /debug?name=hint for the mode. All
  four clients ran the HEAD build (0.0.69-5-g6c939963) against one `sol
  run --lan` server, the builders' `win32-x64-msvc` and `darwin-arm64`
  for the two desktops.

  | client | sustained | missed presents | first paint of a cold style |
  |---|---|---|---|
  | TV (TPM171E, armv7, Mali-T860, 50 Hz, 2x) | 50 locked, p95 1.6 ms, GPU 6.2 ms/frame | 0 in 5 s | 83 ms (layout 24: 150 word shapes, paint 59: 27 layers) |
  | tablet (SM-T500, arm64, Adreno 610, 60 Hz, 1.5x, light hinting) | 60 locked, p95 2.3 ms, GPU 12 ms/frame | 0 in 5 s | 27 ms (paint 27: 24 layers, hinted) |
  | Windows (RTX 3070, ANGLE D3D11, 60 Hz, 1x) | 60, p95 0.2 ms, GPU 0.7 ms/frame | 0 in 5 s | under the window's noise |
  | desktop (Intel RPL-P, 60 Hz, 1.33x) | 60 | 1 in 3 s at load | - |

  The hinter on the armv7: switching the TV to the light mode at 2x
  (every 16 px cell remade hinted, the font's glyph styles derived on the
  UI thread) cost one 80 ms frame (paint 79, 27 layers), the same as the
  unhinted cold field's 83 and more than the hinted cold field's 64
  (the words already shaped), so the styles derivation is not visible
  beside the cells themselves and stays where it is (tiny.md keeps the
  note). The 2x TV crop went to the user for the look step 4 owed.
  Windows: the layer draws through ANGLE's D3D11 (one `text-atlas` rgba8
  1024 square in `/gpu`, 38 draws, no paragraph ops), light and full
  both paint. macOS: the dev client renders headless over ssh with no
  console session (`sol render --project` on the builder's checkout: the
  offscreen driver has no EGL device enumeration on ANGLE, so the bare
  EGL pbuffer context takes over, Apple M1, "OpenGL 4.1 Metal"), the
  1x frame crisp and hinted like the desktop's, which is the way to see
  any rendering change on the Mac from here. The tablet (the one client
  where the light hinter is new since step 4's read; it sat behind its
  pattern lock until the user unlocked it, the player pauses behind the
  keyguard) holds 60 with nothing missed, a hinted cold style's first
  paint is one 27 ms frame, and its 1.5x crop went to the user with the
  TV's. The user's read of both: the text as it is looks perfect on both
  devices, so the DirectWrite blend at 2x unhinted and the light hinter
  at 1.5x pass, and the plan closes.

