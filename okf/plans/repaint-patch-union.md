---
title: The partial-repaint patch on the mover probes is a third of the window
description: On the dense and sparse mover probes the window's repaint patch reaches from the mover's older left edge to the window's right edge and from the top of the window to the mover's bottom, about 410,000 device pixels for a 20 px mover, under Impeller and anvil alike; the route's union of this frame's damage with the ring of the last age-1 frames' damage is the suspect, in particular a fold of an empty rect with a non-zero origin.
created: 2026-11-18
---

# The partial-repaint patch on the mover probes is a third of the window

Part of stage 7 of [own-renderer](own-renderer.md): that stage is not done until this item is.

## Symptom

`probes/damage-probe.tsx` and `probes/damage-probe-sparse.tsx` move a
20 px square over a grid of rects. With the window's partial repaint on,
`/stats` reports `damagePx` near 410,000 per frame (1920x1080 TV) and
about 494,000 on the desktop at 1280x720, and the renderer's per-frame
instance count is the grid inside a patch about 1100 by 300 pixels, not
the two quads of the mover. The patch spans from the mover's older left
edge to the window's right edge and from the top of the window to the
mover's bottom. Both renderers show it (recorded in
okf/plans/own-renderer.md, "Stage 2: the measurement"), so it is the
repaint route's, not the renderer's.

## Where to look

`alloy/src/raster/repaint.rs`: the route builds the patch as the union of
this frame's damage with the ring of the last age-1 frames' damage. A
fold that starts from an empty rect with a non-zero origin (or from the
origin itself) would pin one corner at (0, 0) or at the window's edge,
which is the shape seen. The regression test that pins the walk's
rejection outside a patch clip (`alloy/src/tests/anvil.rs`) is not
affected: the walk rejects correctly, the patch is too big.

## Done looks like

The mover probes report a patch of the union of the mover's old and new
boxes, grown by the damage edge, under buffer age 1 and 2; `damagePx`
drops by two orders of magnitude on both probes; a test in
`alloy/src/tests/damage.rs` or `repaint.rs`'s tests folds a ring that
includes an empty rect and asserts the union has no phantom corner. Then
the TV figures for the mover probes in the own-renderer plan are re-read:
they measured this patch, not the mover.
