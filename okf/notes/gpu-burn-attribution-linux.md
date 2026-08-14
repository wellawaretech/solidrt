---
title: Measuring which process burns the GPU, on Linux
description: The /proc fdinfo probe that unravelled the idle-GPU burn, its three caveats, and the compositor knock-on rule - every client present makes the OS recomposite, so an app's idle burn is charged twice.
created: 2026-08-13
---

# Measuring which process burns the GPU, on Linux

Sample `/proc/*/fdinfo` twice, diff each client's `drm-engine-render` busy-ns,
and attribute per process. That is how a 30-50% system GPU burn was traced
while every in-app number looked innocent (jsMs ~0.1, paintMs ~0.4, layout ~0):
the split came out as compositor about half, client about 40%, while the app
was supposedly idle. It is what exposed a standing `onFrame` forcing
render+present every vsync, and then that the runner re-presented unchanged
frames at 60/s with zero app activity.

Three caveats, or the numbers lie:

- Dedupe by `drm-client-id`. The same client appears more than once.
- It overcounts with multiple GL contexts. Only ratios are trustworthy, never
  absolute busy-ns.
- It is Linux/DRM-only. macOS, Windows and Android each need a different
  mechanism, which is why this never became scaffolded guidance.

The rule worth carrying beyond the probe: **every client present makes the OS
compositor recomposite**, so an app's idle burn shows up twice - once in the
app, once in the compositor. A present you did not need costs more than the
frame it drew.

In-app counters answer a different question than this probe does. `gpuPassMs`
in `get_stats` is raster-thread occupancy issuing the passes, not GPU-side
duration - see [gpu-timer-query-pass-timing](../backlog/gpu-timer-query-pass-timing.md).

Source: the deferred cross-platform attribution item, extracted when okf was
restructured 2026-08-13.
