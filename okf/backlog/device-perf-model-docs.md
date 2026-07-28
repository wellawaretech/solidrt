---
type: backlog-item
title: The documented perf model is desktop-shaped
description: "GPU work is nearly free, JS is the slow lane" holds on desktop and mid-range mobile but is wrong by ~8x on TV-class hardware, where the compositor can set the frame budget outright; the scaffold AGENTS.md now carries the device spread and how to find your own numbers.
status: done
timestamp: 2026-07-27T00:00:00Z
---

# The documented perf model is desktop-shaped

## Done 2026-07-27

`packages/cli/scaffold/AGENTS.md` gained a "Where GPU work stops being free"
subsection under the performance model, plus a qualifier on the section's
opening claim pointing at it. Content: the ~8x device spread with the three
measurements below, primitive count as the budget on tiled GPUs (with fill and
target size measuring free), compositor-bound as a distinct condition and how to
recognise it (content-independent floor), the `rasterQueue` / `fenceTimeouts`
signals, and `dumpsys SurfaceFlinger --latency` as ground truth when engine
timings disagree with the screen.

Deliberately left as a subsection rather than more numbered rules: the existing
list is ordered by leverage and these are about device variance, not a
lower-leverage version of the same advice. The original guidance is unchanged -
it is right for the common case.

Note `examples/spin/AGENTS.md` carries an older independently-drifted copy of
this section and was not updated; per-project copies are scaffold-time
snapshots and do not get retrofitted.

Split out of idle-tick-gpu-backlog-runaway.md. Cheapest item from that session
by value-to-effort, and the one that would have prevented the app that
triggered the whole investigation.

The scaffold AGENTS.md "Performance model (JS is the slow lane)" section says
per-frame JS is the expensive path while GPU work is nearly free, and steers
continuous effects into shaders with one uniform write per frame. That advice
is correct on desktop, correct on mid-range mobile, and badly wrong on at least
one shipping form factor - with no hint in the docs that the gap exists.

## The measurements, all 2026-07-27, identical app and runtime

An `examples`-style point-cloud flower: two `createPipeline` passes,
233,600 + 6,532 point-topology vertices, one params write per pipeline per
`onFrame` - exactly the shape the docs recommend.

| device | frame time | |
|---|---|---|
| desktop linux | 16.7 ms | 60 fps locked |
| Samsung SM-T500, Adreno 610, Android 12, 60 Hz | 16.7 ms | **60 fps locked**, `rasterQueue` 0 |
| Philips TPM171E, Mali-T860, Android 8, 50 Hz | 120 ms | 8.3 fps |

**~8x spread on identical work**, and the mid-range 2020 tablet is
indistinguishable from desktop. So the honest lesson is not "mobile GPUs are
weak" - it is that the spread is wide enough that the number has to be measured
on the target, and that one class of device breaks the model's assumptions
outright.

Two device-specific facts worth documenting, both counter-intuitive:

- **On the TV, primitive count is the GPU budget and fill is free.** Frame time
  against total vertices: 20k -> 80 ms, 34,800 -> 100 ms, 100k -> 380 ms (all
  with a trivial pass), while `gl_PointSize = 3.0` (9x the fill) measured
  within one vsync of 1.0, and rendering into a quarter-size target measured
  identical to full size. A tiler charges per primitive regardless of pixels
  covered. Nothing in the current docs suggests primitive count is a budget at
  all.
- **On that same TV the compositor sets the budget, not the GPU.** A near-empty
  20k-vertex scene still presents only every 80 ms because `eglSwapBuffers`
  blocks 4-5 refresh periods (see android-surface-swap-latency.md). Between a
  near-empty scene and 233k shaded vertices there is 40 ms of difference against
  an 80 ms swap - so on this hardware tuning vertex counts is close to
  pointless, which is the opposite of the advice the first bullet implies.

## What to write

- Keep the existing JS-is-the-slow-lane guidance; it is right for the common
  case and the shader-over-elements advice stands.
- Add that GPU-free is a desktop-and-modern-mobile property, not a universal
  one, with the spread above as the evidence.
- Add primitive count as a budget that exists on tiled GPUs, and that fill /
  target size are usually not the lever people reach for first.
- Point at the tools for finding your own numbers: `get_stats` (`fps`,
  `frameMs`, `rasterQueue`, `fenceTimeouts`) and, when those disagree with what
  the screen is doing, `dumpsys SurfaceFlinger --latency <layer>` for ground
  truth. Worth saying explicitly that engine-reported timings can be honest and
  still not describe the frame period - that is what happened here.

Related: android-surface-swap-latency.md, gpu-pass-timing.md,
idle-tick-gpu-backlog-runaway.md (the full session data).
