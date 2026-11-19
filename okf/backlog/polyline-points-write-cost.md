---
title: The cost of writing a polyline's points every frame
description: Rewriting thirty polylines of 64 points a frame costs 19 to 21 ms of UI thread on the armv7 TV (54 ms at 256 points), 7 to 10 microseconds per point against 1.4 ms on the desktop; find where the points write path spends it (the marshal of the Float32Array, the extent and path build, the list rebuild) and bring a live chart back under the frame.
created: 2026-11-19
---

# The cost of writing a polyline's points every frame

`<line points>` exists so a trace that changes every frame is one array
write (`packages/core/examples/line-points.tsx`). The stage 4 TV read of
the path bench (okf/plans/own-renderer.md) priced that write on the
Philips TV: thirty polylines of 64 points rewritten each frame hold the
UI thread for 19 to 21 ms per frame (the live route's `p50Ms`), 54 ms at
256 points, where the desktop spends 1.4 and 3.2 ms. The renderer is not
in it: the raster thread tessellates the same strokes in about 3 ms.
`probes/write-path-bench.tsx` is the older probe of property writes.

## Done looks like

- The per-point cost on the TV is known by stage: the JS to native
  marshal of a typed array, `extent()` and `polyline_path` (a lyon path
  built per write), the dash check, the list rebuild of the panel.
- The live route at 64 points holds 50 fps on the TV with the UI thread
  under 60 percent; 256 points under the frame.

## Involves

- A measurement with the stats' `jsMs` and `paintMs` split, and a
  counter or a trace around the points setter in `kinds/line.rs` and its
  flux binding.
- Likely moves: build the lyon path straight from the slice without the
  `Pen` round trip; skip `extent()` when the layout form does not need
  it; keep the panel's list rebuild to the one node.
