---
title: A full glyph atlas drops glyphs instead of evicting
description: The glyph engine's atlas grows by doubling up to the device's texture size cap and then refuses further cells (a warning, the glyph stays missing); a wide repertoire that keeps turning over - a terminal, a CJK document - needs eviction of cells no live run references, which means knowing who references what.
created: 2026-10-06
---

# A full glyph atlas drops glyphs instead of evicting

## Symptom

`GlyphAtlas` (alloy/src/rendertree/text/glyphs/atlas.rs) grows by
doubling the smaller side, repacking from its CPU mirror, until a side
would pass the cap `flux:font` sets from `GpuLimits::max_texture_size`.
Past that, `insert` says `Full`, the plugin logs a warning once per glyph
and marks the glyph failed, and every run over it draws nothing at that
glyph's advance forever. At 48 texels per em a 2048-square sheet holds
about a thousand msdf cells; a terminal grid or a long CJK text turns
over more than that, and a label font never does. Today's contract is
right for labels and wrong for the terminal the mask kind exists for.

## Done looks like

Cells that no live consumer references are evicted before growth is
refused, WebRender's texture-cache policy: the atlas keeps a reference
count or a last-used stamp per cell, a consumer tells the font which
glyphs it holds (a text run on placement, a terminal grid per frame),
and a full atlas frees the least recently referenced unreferenced cells
and repacks. Eviction moves cells, so it rides the same re-frame path a
growth does (`SpriteFont` re-reads every placement when the atlas size
changes; a repack without a size change needs the same signal, so the
atlas should report "placements changed" rather than "size changed").

## Involves

The atlas: an `AllocId` per cell again (dropped as unused), a reference
or stamp per cell, `release(glyph)`/`touch(glyph)` and an eviction pass.
`flux:font`: a release surface (`releaseGlyphs(font, ids)`) and the
"moved" signal to JS. `@solidrt/2d`'s font: release on run destroy and
re-set, and a re-frame on "moved" (today on size change).
