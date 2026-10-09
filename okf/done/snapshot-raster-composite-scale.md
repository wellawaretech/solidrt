---
title: Snapshot boundaries are resampled under a scale
description: A snapshot boundary rasterizes at the display scale only, so under a design-size fit or a scale transform its texture is scaled on composite and every pixel in it is resampled, however well its quad lands on the grid; the text layer already rasterizes at the display scale times the chain's scale and tolerates drift, and the snapshot should do the same.
created: 2026-10-08
completed: 2026-10-09
---

# Snapshot boundaries are resampled under a scale

## Symptom

`boundary::snapshot_node` rasterizes a snapshot boundary at
`platform.display_scale()` and composites the texture as a quad of the
node's logical size. Under an ancestor scale (a design-size fit, which
the TV apps use; a scale transform) the quad is scaled on composite and
the texture is resampled: a 1 px line inside reads soft even though the
quad's edges are on the device grid (okf/done/pixel-snapped-paint-boxes.md
put them there). The text layer does it right: `Text::build_layer` reads
its raster density from the grid map (display scale times the chain's
scale) and re-rasterizes past a 2% drift (`LAYER_SCALE_TOLERANCE`), so a
zoom costs a re-raster from its second frame and float noise costs
nothing.

## Done looks like

A snapshot's raster density follows the grid at its slot, with the text
layer's tolerance: under a 1.5x design-size fit at 1x the texture is
1.5 device pixels per logical pixel and the composite is pixel-exact. The
`SnapshotKey` carries that scale (it already keys on `scale`), storage is
re-rendered in place at a dimension match as today, and a scale
animation re-rasterizes at the tolerance's pace, not per frame.

## Involves

`boundary.rs`: `snapshot_node_unculled` takes the scale from
`grid::scale(&ctx.grid)` (the slot grid, before the own matrix) instead
of the display scale, with the tolerance deciding reuse; the raster grid
(`grid::raster`) and the shader outset follow; `service_captures` stays
at the display scale (a capture is the node at its own density).

## Done

Landed 2026-10-09 with [text-layer-motion](text-layer-motion.md): the
snapshot's density is the grid inside its own matrix (`inner_grid` in
`snapshot_node_unculled`), followed by the raster hysteresis every text
layer and recording boundary use (`text::raster_density`): pixel-exact
where the node rests, kept and stretched while its scale moves, made for
a known zoom's end at once. The key carries the scale as before, so
storage re-renders in place at a dimension match. `service_captures`
stays at the display scale.
