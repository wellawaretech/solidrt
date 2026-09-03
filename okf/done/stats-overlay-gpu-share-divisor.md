---
title: The overlay's GPU% divides per-present cost by the per-tick period
description: gpu_ms is GPU execution per PRESENTED frame while frame_ms is the gap between render-handler TICKS, so the HUD's GPU share inflates in exact proportion to how well the demand gate works - a settled app with the GPU at 1.3% busy reads GPU 50%, and the reader concludes the opposite of the truth.
created: 2026-09-03
completed: 2026-09-03
---

# The overlay's GPU% divides per-present cost by the per-tick period

## Symptom

An idle app whose demand gate is working perfectly - one present per
second, 59 frames skipped - reads `GPU 50%` on the stats HUD at `fps 1`.
Ground truth over the same window, read from the kernel
(`drm-engine-render` in `/proc/<pid>/fdinfo`, i915): 1.3%.

The error is not noise and not a smoothing artefact. It scales with how
much work the demand gate saves, so the figure is worst on the
best-behaved apps and only reads correctly on an app that presents every
tick. A reader chasing a GPU problem on a settled app finds one that does
not exist.

## Mechanism

The numerator and the denominator count different events.

- `gpu_ms` is GPU micros over the delta of the **present index**
  (`lattice/src/stats.rs`, the `gpu_mark`/`gpu_now` pair folded in
  `refresh()`): `record_gpu(render_frame.frame, ..)` stamps the raster
  thread's cumulative `frame_exec_micros + pass_exec_micros` against
  `RenderFrame::frame`, which `lattice/src/runtime.rs` documents as "the
  present index the frame being computed would get". So `gpu_ms` is GPU
  ms **per presented frame**.
- `frame_ms` is the smoothed gap between calls to `record_js`
  (`lattice/src/stats.rs`), which `lattice/src/plugins/draw.rs` calls on
  every frame the JS thread sees, **before** the demand gate, since flush
  runs even when the draw is skipped. So `frame_ms` is one **tick**
  period, i.e. one vsync.
- `push_hud_lines` (`lattice/src/overlay.rs`) then forms every share with
  the same `pct(ms) = ms / frame_ms`, including `pct(gpu_ms)`.

For the JS and native-draw phases that is right: those are per-tick costs
against a tick period, and the block comment above `pct` says so. The GPU
line is the one figure in the group that is not per-tick, and it inherits
the divisor anyway. At one present per second against a 16.7 ms tick, a
whole second of GPU work is divided by one vsync.

## Shape

Divide the GPU line by the present interval rather than the tick period.
The present interval is already derivable where `gpu_ms` is computed -
the same `(frame, micros)` pair carries the present delta, so the wall
time spanned by those presents is the natural denominator, and the
figure becomes GPU busy over the window it was measured on.

Keep it a share of the same visual scale as the phases (the comment
promising "near 100% the GPU is the bottleneck" should stay true), but
say plainly in the comment that this share has a different denominator
from the four above it, so the next reader does not re-unify them.

## Check while in here

Two adjacent figures were reported wrong in the same sitting and both
have moved since, so re-verify rather than assume:

- `gpuFrameExecMs / timeMs` as a cumulative share. The `get_stats` tool
  description no longer invites that division (it now says to difference
  two queries, and points at the window's `gpuFrameExecMsPerFrame`), so
  this may already be closed by documentation.
- `window.gpuFrameExecMsPerFrame` reading 0 for an app presenting every
  tick. The ring records reused frames when `content_changed`
  (`lattice/src/plugins/draw.rs`), which covers the GPU-content-changes-
  without-a-rebuild case that was reported failing, and
  `frame_exec_ms_per_frame` divides by the present delta, not the record
  count (`lattice/src/frame_history.rs`). A residual is possible from
  the timer queries lagging a frame or two across a two-record slice.

## Done looks like

A settled app - display list fully reused, one present per second, GPU
idle - reads a single-digit GPU share on the HUD, and an app whose GPU is
genuinely saturated still reads near 100%. Both checked against
`drm-engine-render` on Linux.

## Resolution (2026-09-03)

`Stats::sample` (`lattice/src/stats.rs`) now derives a second figure from
the same `(frame, micros)` pair it computes `gpu_ms` from: `present_ms`,
the window's wall time per presented frame. `push_hud_lines`
(`lattice/src/overlay.rs`) divides the GPU line by that instead of
`frame_ms`, with the comment saying so; the four phase shares keep the
tick period. Since both figures span the same sample window, the share
is the window's GPU busy fraction, which is what `drm-engine-render`
measures.

Verified on the Linux desktop client (i915), HUD against the kernel over
the same 6 s:

| app | presents | HUD before | HUD now | drm-engine-render |
|---|---|---|---|---|
| hello-world, settled (1 reuse, 59 skip) | 1/s | 50% | 0% | 0.16% |
| trails, 1280x720 | 61/s | 12% | 12% | 12.8% |
| trails, fullscreen 1692x1128 | 62/s | 24% | 24% | 25.4% |

The every-tick rows also cross-check the window figure: 1.98 ms and
4.02 ms `gpuFrameExecMsPerFrame` (plus 0.09 ms of passes) over a 16.4 ms
present are 12.6% and 25%. A saturated GPU was not staged; the share is
GPU micros over wall micros, so saturation reads 100% by construction.

The two adjacent figures re-checked:

- `gpuFrameExecMs / timeMs`: closed by documentation. The `get_stats`
  description says to difference two queries and points at the window's
  per-frame figure; nothing in the payload invites the cumulative share.
- `window.gpuFrameExecMsPerFrame` reading 0 for an every-tick presenter:
  not reproducible. trails (GPU content changes through a reused display
  list, no rebuilds) reports 1.98 ms over a 5 s window of 299 frames.
