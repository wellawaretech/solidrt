---
title: The sprite font's side of glyph atlas eviction
description: The glyph atlas evicts at its cap and the text atlas uses it, but flux:font never turns it on, so a sprite font's full atlas still drops glyphs; turning it on needs a release surface and a move signal from fontAtlas, since a same-size eviction repack moves cells under the runs without a re-frame.
created: 2026-10-06
---

# The sprite font's side of glyph atlas eviction

## Where it stands

The atlas evicts. `AtlasPacker::with_eviction(frames)`
(alloy/src/rendertree/text/glyphs/atlas.rs) lets an owner that stamps its
frames (`begin_frame`, `touch`) lose cells unused for that many frames:
at the cap, `insert` evicts them, always repacks, and answers `Moved`
(or `Full` when the cell still does not fit). The text atlas every
`<text>` draws from turns it on (`EVICT_AFTER_FRAMES` in
`glyphs/text_atlas.rs`), stamps its frames, and counts any move, an
eviction included, in its generation, which re-records every text that
holds placements.

`flux:font` (flux/src/alloy_plugins/font.rs) does not. Its atlas is
`AtlasPacker::new(kind, max_side).with_format(Rgba8)`, without
`with_eviction`, and it never calls `begin_frame` or `touch`. Eviction
never fires for it: past the cap `insert` says `Full`, the plugin logs a
warning and marks the glyph failed, and every run over it draws nothing
at that glyph's advance. At 48 texels per em a 2048-square sheet holds
about a thousand msdf cells; a terminal grid or a long CJK text turns
over more than that, and a label font never does.

## What blocks turning it on

A move signal. `fontAtlas` reports the texture, the width and the height,
and `glyphCells` reads a placement on demand; nothing counts moves.
`@solidrt/2d`'s font (packages/2d/src/font.ts) re-reads every placement
and re-frames its runs only when the width or the height changed
(`grew`). Today every move of a `flux:font` atlas is a growth, so that
holds. An eviction repack keeps the size and moves the cells, so with
eviction on, the runs would keep drawing old placements from the new
texture.

A release surface. The font cannot tell which glyphs a JS run still
holds: a cell a live run draws must not go, and stamping a use per frame
from JS for every glyph on screen is the cost to avoid.

## Done looks like

- `fontAtlas` reports a move count (or a generation) that grows on every
  `Moved`, a growth or an eviction repack, and `@solidrt/2d` re-reads its
  placements and re-frames on a change of that, not of the size.
- `flux:font` has a release surface (`releaseGlyphs(font, ids)`, or a
  per-glyph reference count fed from run create, re-set and destroy), and
  evicts only cells no run holds; `@solidrt/2d` releases on run destroy
  and re-set.
- A full atlas with releasable cells evicts and repacks instead of
  failing the glyph; one with none still fails it, as today.
