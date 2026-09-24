---
title: The stats overlay reads GPU 0% while the same counters say 16%
description: Fixed 2026-09-23: the HUD computed its own GPU share from a once-a-second pair of counter marks and read 0% while get_stats, from the frame history, read 16% on the same client; the HUD now takes the frame history's figure over the last second (RasterRates::gpu_share_pct, the one computation, unit-tested), and reads 22% on a sliding-panes probe where the query agrees.
created: 2026-09-10
completed: 2026-09-23
---

# The stats overlay reads GPU 0% while the same counters say 16%

## Symptom

The on-screen stats HUD prints `GPU 0%` consistently, on an app holding
61 fps with a few hundred to a couple of thousand sprites in flight. The
same client's counters disagree. Two `get_stats` samples 138 s apart:

| | A | B | delta |
|---|---|---|---|
| `frame` | 60983 | 69260 | 8277 presents |
| `gpuFrameExecMs` | 130792 | 150845 | 20053 ms |
| `gpuPassExecMs` | 5835 | 8063 | 2228 ms |
| `timeMs` | 2656034.23 | 2794042.74 | 138008.51 ms |

Those are exactly the inputs `overlay::push_hud_lines` divides:
`gpu_ms = (frame_exec + pass_exec) / presents = 22281 / 8277 = 2.69 ms`,
`present_ms = 138008.51 / 8277 = 16.67 ms`, `gpu_pct = 16%`.

The figure matters more than its size: it is the one line on the HUD that
says the GPU rather than the JS thread is the limit, and a sprite or
shader app at scale is exactly where that question is live. `get_stats`
answers it correctly, so only the HUD is wrong.

## What inspection rules out

Both paths compute the same quantity the same way. `Stats::sample`
(lattice/src/stats.rs) takes the delta between `gpu_mark` and `gpu_now` on
the 1 s refresh; `frame_history::summarize` (lattice/src/frame_history.rs)
takes the same delta over the query window; both are `d(a, b) / 1000.0 / n`.
`record_gpu` runs every frame before `record_js`
(lattice/src/plugins/draw.rs), so `gpu_now` is fresh, and although
`overlay_due()` is read before `record_js` triggers the sample, the
snapshot the overlay is built from is taken after it, so the HUD is not
rendering a stale window.

`gpu_ms` and `present_ms` are assigned together inside the same
`f1 > f0` block, so a printed `0%` (rather than no GPU line at all) means
that block did run and produced a `gpu_ms` under 0.08 ms. That is not a
scaling error, it is a delta of nearly zero: the two timer-query counters
as `record_gpu` observes them are not advancing across a sample window,
while the same counters read through the frame history over the same
window plainly are.

## Done looks like

The HUD's GPU share agrees with the share computed from two `get_stats`
samples over the same window, on a client whose context has timer queries.
A unit test pins `gpu_pct` against a synthetic counter sequence, since the
arithmetic is small and the failure is silent.

## What it involves

Instrumenting a build rather than reading further. The two inputs to
check:

- `gpu_mark` / `gpu_now` and the `f1 > f0` gate: log both pairs at each
  sample. If the frame index recorded alongside the counters does not
  advance the way the refresh assumes, the block keeps whatever it last
  computed, and a first window that produced ~0 sticks forever.
- `present_ms`: `wall_delta * 1000.0 / presents` uses the frame index
  recorded with the counters as the present count while the numerator
  counts wall time. The two agree while every frame presents, as here, but
  the demand gate makes them differ in general, and the comment above the
  GPU line says the share is deliberately taken against the present
  interval rather than `frame_ms` for that reason. Worth confirming the
  divisor is the one that comment intends.

## Fixed (2026-09-23)

Not by instrumenting the old path but by deleting it: `Stats` kept its own
`gpu_mark`/`gpu_now` pair, sampled once a second, and divided them itself,
while the stats query divided the frame history's raster samples over its
window. Two computations of one figure, and only the second was right.
`RasterRates` (lattice/src/frame_history.rs) now carries `present_ms` and
`gpu_share_pct()` - the window draw plus the shader passes per presented
frame against the present interval - and the draw loop reads it over the
last second when it builds the HUD; `StatsSnapshot` lost `gpu_ms` and
`present_ms`. A test pins the share against a synthetic record sequence,
and the line is hidden while no frame changes the picture (an idle app has
no GPU share to show). Verified on the linux client: the HUD line moved
from a stuck 0% to 22% on the batch-1 probe while `get_stats` read the
same figure from the same window.
