---
title: Drop what text kept from its layer era
description: Since text draws straight into the frame (own-renderer stage 2), several parts of the text path still follow the layer model; a paint change reshapes the paragraph and redoes the per-glyph work, the atlas holds every glyph twice in CPU memory, and the layer machinery, its names and two backlog notes are left over. Three items, no pixel changes, one step.
created: 2026-11-19
completed: 2026-11-19
---

# Drop what text kept from its layer era

Stage 2 of [own-renderer](../plans/own-renderer.md) made a `<text>` record
its glyph runs as quads from the text atlas, with no rgba8 layer between
the atlas and the frame. Several parts of the text path still follow the
layer model, where a text was rasterized into its own texture and the
texture was drawn. The three items here change no pixels and fit in one
step. The zoom policy the layer era also left behind is a design question
of its own: [text-zoom-distance-field](../backlog/text-zoom-distance-field.md).

## 1. A paint change reshapes the paragraph and redoes the per-glyph work

The shaping cache key (`ParaKey` in `alloy/src/rendertree/text/shape.rs`)
holds the text's `paint`, and each keyed `TextRun` holds its span's
`RunOverrides.paint`. So a color change on a text or a span misses the
key: `prepare_owned` drops the shaped runs and the layouts, reshapes the
whole paragraph, and bumps `generation`. The per-text run cache
(`TextRaster` in `alloy/src/rendertree/text/mod.rs`) keys on that
generation, so it is remade too: the run styles, the line runs and every
quad, which carries its run's color (`GlyphQuad` in
`anvil/src/glyphs.rs`). A color fade, a hover tint or an opacity change
on a text does all of this each frame it animates. On
`probes/text-prose-bench.tsx` the per-glyph part alone was 0.74 ms of
`paintMs` per frame before the run cache, against 0.09 with it; the
reshape sits on top.

With layers the color was baked into the texture, so a paint change had
to re-rasterize. Now it does not need to, and shaping never depended on
it.

Done looks like: `paint` leaves `ParaKey` and the run key (the shaped
runs still carry the style they draw with; the key only decides when to
reshape), and `TextRaster` keys on the layout and the cells, not on the
paint. A paint change then re-records the kept quads by reference. The
color becomes a per-run value (in the op, not in each quad), or a fade
goes through the op's alpha. Measure first: a prose probe whose text
color animates, `paintMs` before and after.

## 2. The atlas holds every glyph twice in CPU memory

`AtlasPacker` (`alloy/src/rendertree/text/glyphs/atlas.rs`) keeps
`sources`, each cell with its own pixels (`Cell.pixels` in
`glyphs/cells.rs`) for a repack, and `mirror`, the whole atlas texel for
texel, which the dirty-rect upload (`copy_out`) and the whole upload
after a growth or repack read. The GL texture is the third copy. The
mirror is the costly one: it is the size of the atlas area, empty space
included, and 4x a mask cell in an rgba8 mask atlas. Under atlas churn
the desktop atlas grew to 16384x8192 R8, which is 128 MiB of mirror.

The change is smaller than it sounds: `repack` already iterates `sources`
and blits each cell into a fresh mirror, so the repack path needs no
redesign. Done looks like: no kept mirror. A dirty rect is one newly
placed cell and uploads from that cell's pixels; a whole upload builds
its buffer from `sources` in `repack` and frees it after the upload.
Context-loss recovery keeps no texture pixels
(`alloy/src/raster/recover.rs`), so the atlas re-uploads on recovery the
way it does after a repack. Steady-state memory then follows the glyphs
held, not the atlas area. The module header ("growth as a repack from the
mirror") and the `mirror()` accessor go with it.

## 3. Leftovers of the layer machinery

- The `text_layers` counter counts nothing since stage 2, yet it runs
  through `alloy/src/rendertree/counters.rs`, `lattice/src/frame_history.rs`,
  `lattice/src/stats_query.rs` (`textLayers` in `/stats` and its window),
  the overlay's TXT line (`lattice/src/overlay.rs`) and the `get_stats`
  description in `packages/cli/src/mcp/main.ts`, which still tells an
  agent it counts the glyph pass's layer rasters. The plan's stage 2
  finding leaves its retirement to stage 7; this pulls it ahead, as the
  glyph pass was, because a stat that misleads an agent through
  `get_stats` is worse than a missing one. Remove it, or re-aim it at
  what costs now (texts whose runs were remade this build), and update
  the plan's finding.
- Stage 7 of the plan still lists `TextLayer` and its retention for
  removal; the type is already gone (it is `TextRaster` now). Drop the
  line from the plan.
- `build_layer`, `draw_layer`, `layer_geometry` and `layer_scale` name a
  layer that no longer exists; the names should say what they do now
  (the text's raster box, its raster density). The comment on
  `OwnedCache.generation` in `shape.rs` ("what a text layer built from
  it checks") and the `TextRaster` doc comment's `build_layer` reference
  go with them.
- [text-layer-raster-stalls-paint](text-layer-raster-stalls-paint.md):
  its cause was the blocking `RasterCmd::RasterizeGlyphs` round trip,
  which is gone. Check it once (a HUD readout updating over a GPU-heavy
  scene, `paintMs` on the frames where the text changed) and move it to
  `done/`.
- [glyph-atlas-eviction](../backlog/glyph-atlas-eviction.md) says a full atlas drops
  glyphs; the atlas now evicts at the cap (`atlas.rs` header, and the text
  atlas counts an eviction as a move). What is left is `flux:font`'s
  side: a release surface (`releaseGlyphs`), and whether JS learns that a
  repack moved placements without a size change
  (`flux/src/alloy_plugins/font.rs` reports the atlas size and reads a
  placement on demand; check that a same-size repack is observable before
  closing). Rewrite the note to that, or close it.

## Outcome

Done 2026-11-19, the three items in one change, reviewed before the
commit. No pixel changed: hello-world and text-flow rendered at 1280x720
before and after differ in 0 of 921600 pixels each.

**Item 1.** Paint and the underline left `ParaKey` (the runs compare by
`TextRun::shapes_like`, the shaping inputs alone), the run's color moved
from each `GlyphQuad` to the `draw_glyphs` op, and `glyph_quads` groups
the line runs by equal paint. A `TextRaster` whose other keys hold
refreshes its paints per build (`paints_current`): a solid color change
recolors the kept runs by reference, a gradient change or two spans that
stop sharing a paint remake them. Groups now draw in text order, where
the solid runs once drew under every gradient run; ink of two paints
overlapping across a span boundary composes in that order.

Measured with a prose probe (40 lines of 16 px prose, the text color
cycling every frame; the control holds the color while a mover demands
the frames), three 10 s windows each:

| color cycling | paintMs | UI p50 | p95 | client cpu % |
|---|---|---|---|---|
| desktop before (RTX 3070, 60 Hz) | 0.70 | 1.25 | 1.42 | 14 |
| desktop after | 0.04 | 0.56 | 0.60 | 8.7 |
| desktop control (color held) | 0.04 | 0.11 | 0.14 | 5.2 |
| TV before (Mali-T860, 50 Hz) | 11.1 | 17.3 | 18.7 | |
| TV after | 1.0 | 7.1 | 8.5 | |
| TV control | 0.7 | 1.4 | 1.9 | |

The TV before-read spent most of its 20 ms frame on the reshape (slow
frames and missed presents in two of three windows); after, it holds 50
fps with none. What remains after (desktop p50 0.56 against 0.11, TV 7.1
against 1.4) is the JS side: 40 signal writes and the rebuild they
damage on the armv7, not paint.

**Item 2.** `AtlasPacker` keeps every cell's pixels (`sources`) and no
mirror: a whole upload builds its buffer from the cells (`texels`) and
drops it, a newly placed cell uploads its own texels (`Dirty::Cells`,
`cell_texels`). That rests on the GPU texture's free texels being zero,
which every path keeps (the first upload is zero-filled, every whole
upload is, and an eviction always repacks); the module header states it.
Steady-state CPU memory follows the glyphs held, one byte a texel for a
mask cell, not the atlas area.

**Item 3.** `text_layers` is `text_rasters` (`textRasters` in `/stats`
and `get_stats`): the texts whose glyph record the walk remade. The
pixel-snap-motion pop test now reads four on mount (each text once in the
mount frame, once more when the style's warm-up lands and grows the
atlas) and none after. `build_layer`, `draw_layer`, `layer_geometry` and
`layer_scale` are `record_glyphs`, `rasterize_runs`, `raster_box` and
`composite_scale`. The plan's stage 7 bullet and stage 2 finding are
updated, text-layer-raster-stalls-paint is in `done/` with its read, and
glyph-atlas-eviction is rewritten to the sprite font's side.
