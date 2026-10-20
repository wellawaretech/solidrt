---
title: Views of one sprite layer far apart from each other see f32 steps in the pose records
description: A sprite layer's pose buffer is one f32 record per sprite relative to ONE render origin, snapped near the primary view's camera; a second view more than about 10^5 px from it (a split screen across a continent) reads records quantized to whole pixels, since no single origin serves both; the fix is records local to region cells, each cell its own buffer and draw, the tile layer's chunking applied to populations.
created: 2026-10-20
---

# Views of one sprite layer far apart from each other see f32 steps

## Symptom

The spatial arena keeps sprite positions in f64 and the sprite layer
uploads its pose records relative to a render origin
(okf/done/3d-large-world-precision.md, `flux:spatial
setInstanceOrigin`), so a layer whose camera is sixteen million pixels
out renders exactly like one at zero. The origin is the PRIMARY view's
(the first live view, views.ts): one buffer holds one record per
sprite, and a record is relative to one point. A second view of the
same layer whose camera is more than about 10^5 px from the primary's
sees the records around its own camera at f32's spacing there - whole
pixels at 10^7 px - and sprites step as they move. Two views that far
apart over one world is a real layout (a split screen over a large map,
a minimap zoomed in on a distant marker), and today the answer is a
layer per view, which doubles the population's writes.

The tile layer has no such limit: its records are chunk-local
(okf/done/2d-baked-layers.md stage B2) and its view stage subtracts the
camera's chunk index before scaling, so every chunk is exact under any
view. The 3d side has none either: an entry's `uModel` is per target, so
each target's origin applies to it.

## Done looks like

Two views of one sprite layer, their cameras 10^7 px apart, both render
their sprites at sub-pixel precision and the population costs one write
per sprite move. The way there is the tile layer's: the sprites of a
region cell (the 2d `<RegionGrid>`, okf/done/2d-region-handlers.md)
hold their pose records in a buffer of that cell, relative to the cell's
corner, drawn as one entry per resident cell per view with the corner's
offset against the view's origin cell carried as exact integers
(`uOrigin`/`uChunk` in tile-pipeline.ts, the spelling to copy). A layer
without regions keeps today's one buffer and one origin.

## Involves

`packages/2d/src/layer.ts` (the pose and style buffers per cell, the
free lists and growth per cell, the pick's slot-to-sprite map),
`views.ts` (one entry per resident cell per view, the per-entry cell
offset), `glsl.ts` (the sprite stage's camera split), `region.ts` (a
cell owns its sprites' buffers), the core's record sinks (a node's
record moves buffers when its sprite changes cell: `retargetRecords` is
per buffer, this wants it per node) and `packages/2d/AGENTS.md` (The
camera, Views).
