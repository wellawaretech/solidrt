---
title: The splat re-sort blocks the UI thread and its spike at 1M is unmeasured
description: A camera turn past the order gate radix-sorts the cloud's key mirror and republishes the id stream inside the flush, on the UI thread; at 1M splats that is a periodic stall the JS p50 hides and no probe has isolated (p95/max per re-sort on the Pixel 7), so the frame-time cost of the in-engine sort - the design's one unmeasured term - is unknown.
created: 2026-09-27
---

# The splat re-sort blocks the UI thread and its spike at 1M is unmeasured

## Symptom

okf/notes/gaussian-splats-against-the-field.md: the in-engine sort is
what puts the viewer ahead of the worker-sorting viewers on latency, but
it runs synchronously in the flush (`rematerialize_retained_order`: an
LSD radix sort over 1M keys plus a 4 MB id materialization) every ~2
degrees of orbit. Every phone table so far reads GPU time and the JS
p50; the stall each re-sort puts on the UI thread has not been measured.

## Measure

- Time the sort and the materialization per re-sort at 100k / 300k / 1M
  on the Pixel 7 (a counter in the order registry, or the frame's
  p95/max through /stats while orbiting against a parked run).
- If the stall is visible at 1M: the sort could run off the UI thread
  (a worker thread owning the key mirror, publishing when done - the
  order then lags one flush, still ahead of a worker sort's several
  frames), or the gate could adapt to the population size.
