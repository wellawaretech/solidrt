---
title: Per-op display-list cost dominates animated frames on slow CPUs
description: The MediaTek TV's ~40 ms animated-frame cost is NOT GPU fill - saturation runs show it scales with static display-list op count at constant damage (800 ops = 26 fps, 50 ops = 50 fps locked) and is identical on the fast path and the rig partial path; the bottleneck is per-op display-list processing on the armv7 CPU, so the real lever is not handing Impeller the full scene DL every frame.
created: 2026-09-02
---

# Per-op display-list cost dominates animated frames on slow CPUs

Started life as "partial repaint on Android's multisampled fast path";
renamed once measurement showed the cost has nothing to do with the
window configuration or partial repaint.

## Symptom, as originally stated (and why its numbers misled)

Measured 2026-09-02 on the Philips TPM171E (MediaTek armv7, Mali-T860,
1080p50), release client, probes sharing an 800-rect static field and a
10 Hz animation (`probes/damage-probe.tsx` / `damage-probe-full.tsx`):

- 20x20 mover: gpuFrameExecMsPerFrame 39.4-40.4 ms
- window-covering animated rect: 44.4-45.4 ms

That read as "fill cost is damage-size-independent, the fast path
redraws everything". The attribution was wrong on two counts. First,
gpu*ExecMs is not defensible on tilers
(okf/done/gpu-timer-attribution.md): TIME_ELAPSED there absorbs
issue time, waits, and DVFS. Second, the honest measurement below shows
the cost is not GPU work at all.

## Measured (2026-09-02, same TV, saturation method)

Method: `/clock?scale=5` drives the 10 Hz probe animation at 50 Hz
(the display's vsync rate); sustained fps = presented-frame delta over
timeMs delta, which is throughput, immune to timer attribution and
DVFS idle-clock artifacts.

| static field | window config | path | sustained fps | frameMs |
|---|---|---|---|---|
| 800 rects | 4x MSAA FBO0 | fast path (full draw) | 26.7 | 36.9 |
| 800 rects | single-sample | rig + patch (partialPresents = every frame) | 26.4 | 38.1 |
| 50 rects | single-sample | rig + patch | 50.0 (locked) | 20.0 |

Damage was a constant 660 px in all three. Conclusions:

- **The window config does not matter.** Fast path and rig-partial
  path are statistically identical. The "give up the multisampled
  config?" question - and any per-app configurability of it - is moot
  on this evidence.
- **The cost scales with display-list op count, not damage.** 800
  static ops push the frame just over the 20 ms vsync budget (hence
  every-other-vsync, ~26 fps at 50 Hz); 50 ops run vsync-locked. The
  UI thread is idle throughout (jsMs 0.3, paintMs 0.06, nodesPainted
  2), cpuPct ~199%: this is the raster thread's CPU-side display-list
  processing. On the rig path the patch's root clip means Impeller
  rejects the ~798 out-of-clip ops per frame - cheap per op, ~30 us x
  800 on this armv7 is ~24 ms. Desktop hides the identical walk behind
  a fast x86; that is why stage 2 looked like a fill win there.
- `probes/damage-probe-sparse.tsx` (50-rect variant) is kept for
  reproducing this.

Earlier same-day findings, still valid:

1. **EGL facts.** The TV's app context lists EGL_KHR_partial_update
   (and EGL_KHR_swap_buffers_with_damage) but NOT EGL_EXT_buffer_age.
   The KHR extension defines the same age attribute
   (EGL_BUFFER_AGE_KHR = 0x313D), so the EXT-only check in
   `raster::buffer_age` was a false negative - fixed to accept either.
   Buffer age returns valid ages every frame; the rig partial path
   works end to end on the TV (partialPresents = every present).
   Side effect: rig-path devices listing only the KHR extension gain
   working partial repaint from the widening (verify the SM-T500
   canary before shipping).
2. **eglSetDamageRegionKHR is useless here.** Wired on the fast path
   (aged union, full frame still drawn), accepted by the driver,
   engaged every presented frame: 37.2 vs 39.0 ms baseline - noise.
   Consistent with the CPU attribution: there is no writeback cost
   worth skipping. The hint code was removed again the same day (the
   `raster::buffer_age` either-extension widening stays - that one is
   a real fix).

## Constraint kept for the record: the rig detour as a DEFAULT

Even before the CPU attribution, rerouting all frames through the rig
was ruled out as a default because it taxes every full frame (video,
scrolls) with an extra fullscreen resolve pass where the budget is
tightest, and the EGL config is a surface-creation decision with no
per-frame switching. The measurement above additionally shows the
detour buys nothing on this device anyway.

## What done looks like now

The dense-field mover probe reaches 50 fps locked on the TV - which,
per the table, means cutting the per-frame display-list walk, not any
GPU-side change. Levers to weigh (discussion first, none started):

- Cull the scene DL to the damage rect when handing Impeller a partial
  frame (alloy knows the patch; per-op cull at DL build/traversal
  level instead of Impeller's per-op clip rejection at encode time).
- Retained per-boundary rasters (texture cache tier direction): a
  repaint boundary whose content is unchanged should not have its ops
  re-walked at all, on any path.
- Re-measure per-op cost on the tablet (Adreno, arm64) to see how much
  of this is armv7-specific before sizing the work.
