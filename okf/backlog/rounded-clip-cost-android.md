---
title: A rounded clip on a box whose size is in flight costs a third of the frame on Android
description: Ten panes with overflow hidden + clipRadius, sliding and resizing on a layout transition, take the Galaxy Tab A7 (Adreno 610, Impeller GLES) from 60 fps to 20 fps - measured from SurfaceFlinger present timestamps, not from get_stats; a static rounded clip is nearly free, one resizing rounded pane costs ~8 ms, ten ~13 ms, and an image under the clip pays most. Cause not yet located below the display list; needs an Impeller-level look.
created: 2026-09-22
---

# A rounded clip on a box whose size is in flight costs a third of the frame on Android

## Symptom

The notes demo (demo/notes: ten frosted panes in a collage, every pane a
`<view overflow="hidden" clipRadius={16}>` sliding and resizing on a `layout`
transition when a note is added or removed) animates at 20 fps on the Galaxy
Tab A7 (SM-T500, Android 12, Adreno 610, release client 0.0.60-10-gb1eec1bf)
with the JS critical path at 5 ms. Removing `clipRadius` from the ten panes,
nothing else, gives 60 fps.

## Measured (2026-09-22)

Method: reload (resets the cadence hold), `dumpsys SurfaceFlinger
--latency-clear`, drive one remove and one add over `send_input` at real
speed (motion slowed 5x, so ~2 s of frames each), then read the layer's
`--latency` history. Interval = present-to-present; GPU span = frameReady
minus queue time (the runtime sets no desired present time, so column 1 is
the queue time). Per-thread CPU from `top -H`. Every row is empty notes,
ten panes, flat tint (no gradient), per-pane frost slice under the clip.

| rounded clips | scene | interval p50 / p90 | GPU span p50 | raster thread CPU |
|---|---|---|---|---|
| none | ten panes reflowing | 17 / 17 ms | 12.8 ms | 30% at 60 fps (~5 ms/frame) |
| one tiny static view (20x20) | ten panes reflowing | 17 / 17 | 12.8 | - |
| one, on the static collage area, all ten panes reflowing under it | reflow | 17 / 50 | 15.8 | 40% at 60 fps |
| one, on one reflowing pane | reflow | 50 / 66 | 21.0 | 15% at 20 fps |
| ten, on the ten reflowing panes | reflow | 50 / 66 | 25.6 | 22-35% at 20 fps (~15 ms/frame) |
| ten, panes static, one edge color fading | static | 17 / 17 | 17.2 | 43% at 60 fps (~7 ms/frame) |
| ten, reflowing, no frost image under the clip (flat fill only) | reflow | 17 / 50 | 18.5 | - |

So:

- A rounded clip is not expensive by itself: a static one, even over the
  whole animated scene, costs a few ms.
- A rounded clip on a box whose size changes every frame (the layout
  slide) is: ~8 ms for one pane, ~13 ms for ten, on top of a 12.8 ms
  frame. The cost is not linear in the count, so part of it is a
  per-frame path change and part per clip.
- What is drawn under the clip matters: with the pane's `d-texture`
  (the frost slice) removed the ten clips cost ~6 ms instead of ~13.
- Both sides pay: the raster thread's per-frame CPU triples (5 -> 15 ms
  at 20 fps) and the GPU span doubles. The UI thread is idle (jsMs
  0.2, paintMs 3).
- get_stats' `gpuFrameExecMsPerFrame` read 43-51 ms in every slow case,
  tracking the held interval rather than the work (see
  ../done/cadence-hold-sticky-android.md, fixed since), so it could not be used for any of
  this.

## Not yet known

Where the time goes below the display list. Candidates, none verified:
Impeller re-tessellating the rrect clip path every frame the box changes
and drawing it stencil-then-cover; the clip's coverage forcing every
entity under it (the image draw above all) off the fast blend path; or
alloy re-recording the clipped subtree (`clip_radius` is baked into
recorded content, kinds/view.rs). A GPU profiler on the device
(Snapdragon Profiler, or perfetto's gpu counters) against the demo's
add/remove is the next step; the demo reproduces it in one tap.

## What done looks like

- The cause named, and either fixed in alloy or documented in
  performance.md as a rule ("a rounded clip on a resizing box costs N ms
  per pane on a tiled GPU; round the paint instead").
- Done 2026-09-22: a `radius` on `d-texture`/`<texture>` (a rounded image
  draw: `draw_rounded_rect` with the texture as an image color source,
  kinds/texture.rs). The demo's ten panes with `radius` on the frost slice
  and a plain rect clip on the card: 121 frames at 17 ms p50 / 17 ms p90,
  GPU span 14.9 ms p50, raster thread ~15% - the same frame as with no
  rounding at all. The rounded clip's own cost stays open.
- Done 2026-09-23: get_stats' `paintOps` counts draws, clips (rounded
  ones apart), save layers and non-source-over blends per rebuild, so
  this kind of cost is attributed in one read instead of a
  reload-and-subtract cycle per hypothesis.
