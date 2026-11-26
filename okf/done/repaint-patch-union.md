---
title: The partial-repaint patch on the mover probes is a third of the window
description: On the mover probes the window's repaint patch reached from the mover's old left edge to the window's right edge and from the top of the window to the mover's bottom, under Impeller and anvil alike; the route's union was sound, and the partner rect was the dev session's badge overlay at the top right, unioned into every frame's damage because the overlay was blended over the whole frame. Fixed 2026-11-26: the overlay draws only inside the frame's patch, and a changed overlay folds its own rectangle in.
created: 2026-11-18
---

# The partial-repaint patch on the mover probes is a third of the window

Part of stage 7 of [own-renderer](own-renderer.md). Closed 2026-11-26.

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

## Cause (2026-11-26)

Not the ring union. Logged along the path on the desktop (a 1280x720
single-sample window, the `Rig` route): the paint walk resolved the mover's
damage to its old and new boxes (30 by 22 logical pixels), the submit
carried exactly that (32 by 24 physical with the pad), and the route's own
input was already the window-wide rect, x from the mover's left edge to
1281 and y from 0 to 302. The term between was the overlay: in a dev
session the raster side always holds one, the "CONN ... FPS" badge at the
top right, and `frame.rs` unioned its rectangle into every frame's own
damage, because the badge is blended over the finished frame and a
re-blend over an aged buffer that already showed it would stack. The
one-rect damage model then unions a 20 px mover at the bottom left with a
badge at the top right into a third of the window. Impeller's client had
the same overlay, hence the same patch under both renderers. The
`damagePx` stat never showed it: it reads the rendertree's damage, before
the overlay term.

## Fix

The overlay's rectangle leaves the per-frame damage. `draw_overlay` takes
the route's patch and blends the overlay only inside it (a scissor on the
composite): the pixels the frame redrew take the badge once, the pixels
outside keep the composite an earlier frame left, which stays right while
the declaration stands. A declaration that arrives, changes its figures,
moves, resizes or leaves folds its old and its new rectangle in
(`overlay_repair`), so the frame that applies it redraws every pixel under
it, and the ring carries that to the aged buffers. On the desktop the mover
probe went from about 570 renderer instances a frame (the grid under the
window-wide patch) to about 100 averaged over a second, the remainder
being the badge's once-a-second refresh, whose frame still unions the
badge with the mover. Smaller than that needs a damage region of more
than one rect, which is not this item.

Still open, as the own-renderer plan's stage 7 reads: the TV figures for
the mover probes measured this patch and are re-read on the device.
