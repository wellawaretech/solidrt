---
title: Gradient fills, non-source-over blends and many small draws each cost a frame's worth on Android
description: On the Galaxy Tab A7 (Adreno 610, Impeller GLES) a linear-gradient d-rect over each of ten panes costs ~15 ms a frame, four tiny destination-out/destination-over draws per pane ~16 ms, and seventy small source-over texture draws ~10 ms - all measured by subtraction from SurfaceFlinger present timestamps. performance.md's "GPU work is nearly free" needs these numbers, and get_stats needs per-frame draw/blend/layer counters so they can be found without a reload per hypothesis.
created: 2026-09-22
---

# Gradient fills, non-source-over blends and many small draws each cost a frame's worth on Android

## Measured (2026-09-21/22, demo/notes on the SM-T500, release client 0.0.60-10-gb1eec1bf)

Same method as rounded-clip-cost-android.md: reload, clear the
SurfaceFlinger latency history, one remove and one add at real speed,
present intervals and frameReady-minus-queue spans from the layer dump.
Ten empty panes reflowing on a layout slide, JS critical path 5 ms.

| pane paint | GPU span p50 | result |
|---|---|---|
| 3-stop linear-gradient `d-rect` per pane (hue into smoke) | ~32 ms | 20 fps |
| the same tint as a flat `d-rect` | 16.3 ms | 60 fps |
| the tint as a 32x32 texture rendered once, stretched over the pane | 21.3 ms | 20 fps |
| four `d-path` corners per pane with `blendMode="destination-out"` plus a `d-texture` each with `"destination-over"` (40 tiny draws) | +16 ms | 20 fps |
| the frost as one full-screen `d-texture` plus 40 gap strips and 30 sheen pieces (70 small source-over texture draws) instead of ten pane-sized slices | +10 ms | 20 fps |

So on this GPU class:

- A gradient paint is shaded per pixel per frame; ten pane-sized ones cost
  more than the whole rest of the frame. A flat fill is the only cheap
  pane-sized paint; even a once-rendered texture stretched over the pane
  costs 5 ms more than a flat fill.
- Any blend mode other than source-over, even on 16x16 draws, is
  catastrophic: 40 of them cost as much as the rounded clips did.
- Draw count matters on its own: 70 small texture draws cost ~10 ms.

## What done looks like

- performance.md gets a "paint costs on tiled GPUs" list with these
  numbers: gradients, rounded clips on resizing boxes, non-source-over
  blends, draw count.
- Done 2026-09-23: get_stats reports `paintOps` per rebuild (latest, and
  the window's worst frame): draws (of which paragraphs), clips (of which
  rounded), save layers, non-source-over blends and gradient paints, on
  the HUD as DRW/CLP/LYR and BLD/GRD, so an app author reads the cause
  instead of bisecting the scene.
- Worth checking in Impeller: whether the gradient is regenerated (a
  gradient texture upload) per draw per frame on GLES, and why
  destination-out/over leave the fast blend path.
