---
title: GPU timer stats are unusable on tiled GPUs, and gpuFrameExecMs can return garbage
description: On Adreno the per-pass execMs and gpuFrameExecMs figures move with unrelated state, invert against ground truth, and on a frame with no passes report 401 ms of GPU time in a 17 ms frame, so anyone optimising from them is led the wrong way.
created: 2026-08-27
completed: 2026-09-02
---

# GPU timer stats are unusable on tiled GPUs, and gpuFrameExecMs can return garbage

Two symptoms with one investigation behind them. The first is a measurement
model that does not survive a tiler; the second is a plain defect that shows
up regardless.

## Symptom 1: the numbers contradict ground truth

Measured on an Adreno 610 (Samsung SM-T500, Android 12, GLES 3.2) running
[packages/3d/demos/src/the-third-dimension.tsx](../../packages/3d/demos/src/the-third-dimension.tsx),
which draws six offscreen passes and composites three `d-texture` leaves.

`gpuFrameExecMs` reported 8.4, 11.5, 18.6 and 22.2 ms for the same composite
work, varying with state that cannot affect it:

- Switching shadow casters off moved it from 8.4 to 18.6 ms while pass exec
  fell and the frame got faster.
- Attaching a second client (a desktop window on the same server) moved it
  from 22.0 down to 8.5 ms.
- Adding `fullscreen` to `<window>` grew the composited area by 10 percent
  and the figure HALVED, 18.3 to 8.35 ms.

The per-pass `execMs` values drift the same way and can invert against the
frame time outright: at renderScale 0.35, turning the three casters off took
the app from 24.3 to 36.3 fps while the summed GPU total went UP, 17.0 to
21.7 ms.

Ground truth for the same app, obtained by subtraction (change one variable,
take the delta in wall-clock frame time):

| | timer says | actually |
|---|---|---|
| window composite, 3 textures + text at 2000x1200 | 8-22 ms | 1.9 ms |
| one render pass, 128x128, constant shader | ~1 ms | 2.15 ms |

## Symptom 2: 401 ms of GPU time in a 17 ms frame

With zero GPU passes running - a window drawing one full-window `d-rect`,
nothing else - `gpuFrameExecMs` advanced by about 401 ms per frame while the
frame took 17 ms and the display was capped at 60 Hz. The value was stable
across four different window contents, which rules out noise. Twenty-three
seconds of GPU time per wall-clock second is not a misattribution, it is a
bad read: a harvest against a query that never completed, a stale or
uninitialised accumulator, or a unit error on the no-pass path.

Reproduced with a since-removed floor-probe demo. A later re-measurement
session (2026-08-27, same tablet) could not reproduce the 401 ms reading -
the worst seen was 94% of wall clock - but did confirm the total is not
defensible on Adreno either: 66% GPU on a frame that is all GPU by
subtraction, 94% on a single fullscreen rect, three different readings for
the same work depending on what ran before.

## Cause

For symptom 1, the measurement model does not hold on a tiled deferred
renderer. `PassTimer` brackets command submission, but a tiler defers the
actual tile execution for an offscreen target until its results are needed -
typically when the window draw samples it. That work then lands inside
whichever timer query happens to be open, so the split between
`pass_exec_micros` and `frame_exec_micros` is decided by draw order and by
what samples what, not by where the work belongs. Every anomaly above is
consistent with this: the figure tracks how much deferred tile work the
window draw pulled in, which is why it falls when there is less to pull and
rises when there is more.

Symptom 2 is separate and wants a straight read of the harvest path in
[alloy/src/raster/mod.rs](../../alloy/src/raster/mod.rs) around
`frame_exec_micros`, since a no-pass frame is exactly the case where a
query may be harvested without ever having been issued.

## Why it matters

These figures are not diagnostics of last resort. They are what `/stats`
serves, what the MCP tooling reports, and what the on-screen overlay shows,
so they are the first thing anyone reads when an app is slow. During this
investigation they pointed at the window composite as a 22 ms bottleneck; it
is 1.9 ms. Acting on that number would have meant rewriting the compositing
path to fix a cost that was never there.

It has since happened for real: on 2026-09-02 the same counter on the
MediaTek TV produced a "~40 ms GPU fill per animated frame" reading that
spawned a mis-attributed backlog item, an eglSetDamageRegionKHR probe
implementation (later removed), and a window-config design debate, before
saturation measurement showed the cost was CPU-side display-list walking
(okf/notes/tv-gpu-measurement-postmortem.md,
okf/backlog/display-list-op-cost.md).

## Resolution (2026-09-02)

The timers now say they cannot be trusted where they cannot, and the
verdict is measured per device rather than assumed per vendor - a renderer
string blocklist was considered and rejected, because it names suspects
instead of catching the crime and gets every unlisted driver wrong in one
direction or the other.

- **Attribution self-test** at raster startup
  ([alloy/src/gpu/timing.rs](../../alloy/src/gpu/timing.rs)): pass A
  renders a deliberately expensive shader offscreen inside one
  `TIME_ELAPSED` query, pass B samples the result through a trivial shader
  inside a second, `glFinish` closes the run. An honest driver books the
  heavy work to A's query; a driver with the deferral pathology books it
  to B's, and A's share collapses - the exact failure above, in miniature.
  Three runs, majority verdict (absorbing DVFS ramp-up), disjoint runs
  discarded, a zero total votes broken. A failed verdict disarms
  `PassTimer`, so `/stats`, MCP `get_stats` and the HUD all report the
  fields absent through the existing `timer_queries: false` path. Mesa
  Intel measures shares of 0.94-0.95 and keeps its timers; the Adreno
  deferral moves the split exactly this way, though the probe has not yet
  been re-run on the device.
- **Wall-clock bound at harvest** (symptom 2, structurally): every pending
  query carries its begin `Instant`, and a harvested result whose GPU time
  exceeds the wall clock between begin and harvest is dropped as a bad
  read. No configuration can report more GPU time than wall time again.
- **Docs**: MCP `get_stats` description and
  [packages/cli/agents/debugging.md](../../packages/cli/agents/debugging.md)
  now say the `gpu*ExecMs` fields are absent when unsupported or when the
  self-test disarmed them (measure by subtraction there), and that
  configuration comparisons divide a frame-counter delta by a `timeMs`
  delta rather than reading the `frameMs` EMA, which disagrees with the
  counters under bimodal frame times (48.8 ms EMA against 40.8 ms
  measured over the same window).
- `alloy/examples/timer_attribution_probe.rs` boots the raster thread with
  the logger installed and prints whether the timers survived; per-run
  shares at `SRT_LOG=debug`.

True per-pass attribution via `GL_TIMESTAMP` around a forced `glFlush` per
pass was deliberately not pursued: the flush changes what is being
measured, and absence plus subtraction answers the questions that matter.
