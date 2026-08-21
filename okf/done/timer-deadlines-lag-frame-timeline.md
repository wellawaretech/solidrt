---
title: Timer deadlines lag the wall clock under frame gating
description: In a GUI app, setTimeout rides the frame-stepped virtual timeline, so with gated or slow frames a 1.5 s timer observably fires ~0.5-1 s late - async UI (pending buttons, toasts, polls) feels laggy while the app idles or animates lightly.
created: 2026-08-19
---

# Timer deadlines lag the wall clock under frame gating

## Symptom

In a GUI app, `setTimeout(fn, 1500)` fires noticeably later than 1.5 s of
wall time. Measured 2026-08-19 (probes/stage3-probe.tsx, release client,
desktop Linux): a 1500 ms timeout backing an async Button action settled
roughly 2.3-2.9 s after the press, i.e. ~0.5-1 s late, while the only
animation running was a small spinner. In an earlier variant of the same
probe the timer appeared to fire only when the NEXT input event arrived.
Anything async-flavored in the UI inherits the lag: pending-button spinners
run long, toasts linger, debounces and polls stretch.

## Mechanism (suspected)

GUI timers deliberately do not use the tokio wall-clock path: lattice
installs virtual time (`install_virtual_time` /
`flux/src/standards_plugins/time.rs`), and `advance_virtual_time` runs per
frame on the flux Timeline, which in run mode is the paced frame clock
(`lattice/src/paced_clock.rs`, frame count times refresh period, corrected
toward wall time; see notes on the frame-clock pacing decision). Two
consequences follow directly:

- Timer resolution is one advance quantum, documented and fine.
- When frames are gated (demand-driven rendering with an idle tree) or slow
  (the probe logged 46.5 ms paints), the frame timeline falls behind the
  wall clock, and every timer deadline drifts with it. The correction toward
  wall time bounds the drift but visibly does not eliminate it.

The pacing itself is a deliberate decision (deterministic rAF/video sync;
do not revert it). The gap is that `setTimeout` semantics silently changed
with it: an app-facing wall-time promise now rides a timeline that only
moves when frames do.

## What done looks like

A decided, documented contract for GUI timers, and the lag either gone or
deliberate:

- Either the event-wait loop wakes at the earliest pending timer deadline
  even when no frame is requested, and an advance runs to wall time on that
  wake (timers stay frame-quantized while frames flow, but never wait on
  one that is not coming);
- or timers knowingly stay on the paced timeline and the divergence is
  documented in flux-types (`setTimeout` docs) with guidance for wall-time
  needs.

A probe that presses an async button and measures fire-time error against
the wall clock, idle and under animation, is the acceptance test.

## Pointers

- `flux/src/standards_plugins/time.rs` (virtual timer queue, advance)
- `lattice/src/paced_clock.rs`, `lattice/src/runtime.rs` (timeline drive)
- `okf/done/idle-tick-gpu-backlog-runaway.md` (tick gate + load shedding:
  the machinery that legitimately suppresses frames while idle)
- `probes/stage3-probe.tsx` (reproduces via the async Save button)

## Resolution (2026-08-21)

The first option shipped, in its minimal form: timers keep advancing once
per frame signal (quantization, pause/step/scale and playback determinism
all unchanged), but in run mode they advance against a new wall-anchored
reading instead of the smoothed animation reading.

Mechanism of the lag, pinned down: the paced clock advances one refresh
period per frame signal and corrects toward wall time at GAIN 0.05, so
whenever signals arrive slower than the refresh cadence the reading
settles toward a lag of (signal period - refresh period) / GAIN, sawtoothing
against the present model's 500 ms stall snap. At the probe's 46.5 ms
paints that is the observed ~0.5-1 s. Timers rode that reading.

What changed:

- `lattice/src/paced_clock.rs`: PacedClock carries a second reading,
  `timer_now_ms` - raw wall time minus paused/scaled stretches, no
  smoothing. Unlike the animation reading it does not skip suspensions:
  timers due across a background stretch fire on the resume tick,
  browser-style (intervals still collapse to one fire per advance).
- `lattice/src/runtime.rs`: `advance_virtual_time` gets the timer reading;
  rAF/render/video keep the animation reading. Playback mode passes the
  deterministic frame clock to both, unchanged.
- `lattice/src/lib.rs`: the `install_virtual_time` seed comes from the same
  timer reading, so a reload does not seed timers on a timeline offset from
  the one that advances them.
- `lattice/src/tests/paced_clock.rs`: unit tests for wall tracking under
  46.5 ms signals (animation reading lags >300 ms, timer reading exact),
  pause/resume continuity, and the suspension-policy split.

Verified 2026-08-21 against a rebuilt release client with
probes/timer-deadline-probe.tsx (setTimeout(1500) fire error vs
performance.now, four trials per phase): idle -7..+32 ms, light animation
+1..+16 ms, heavy frames (40 ms busy-wait per frame) -23..+8 ms - while the
animation timeline in that heavy phase fell 600 -> 2546 ms behind the wall
clock, the lag timers used to inherit.

Second pass, same day, after double-checking the residuals:

- Cold-start early fire (a regression the first pass introduced, caught in
  review before it shipped): the paced clock's first tick only happens once
  an engine is live, so its raw reading includes the whole bundle/eval
  startup stretch - the wall-true reading jumped over it and every timer
  registered at module init (seeded at 0) fired on the first frame. Fixed:
  the first tick anchors the timer timeline instead of living through
  startup (PacedClock::started).
- Early fires from stale deadline bases (up to one frame): deadlines
  anchored to the previous advance's reading. Fixed with at-least-delay
  anchoring: flux's virtual time takes an optional schedule-time now source
  (set_virtual_now_source), and lattice installs PacedClock::timer_live_ms
  over the shared wall origin (run mode only - playback keeps last-advance
  anchoring for deterministic replay).
- The contract is now documented in
  packages/flux-types/standards/time.d.ts: frame-quantized, wall-accurate,
  at least the delay, at most one frame late; suspended-due timers fire on
  the resume frame.

Re-verified with the probe (which gained a module-eval boot timer for the
cold-start case): boot err +31.6 ms (late, not early - startup anchored
away), idle +8..+30 ms, spin +8..+17 ms, heavy +4..+42 ms - every error
non-negative and within one frame signal of its phase's cadence, with the
animation timeline again seconds behind in the heavy phase.

Remaining residual, deliberate: under a suppressed-tick raster backlog
timers stall with the frame signals. The wake-at-earliest-deadline half of
option one stays unbuilt on purpose - waking into a saturated JS thread to
run timer work is exactly the feed-the-backlog loop the tick gate exists
to prevent (okf/done/idle-tick-gpu-backlog-runaway.md), and wall-true
advance already fires everything due on the first tick after the queue
drains. Likewise kept: the dev-scale (set_time_scale) timer advance stays
period * scale in lockstep with the animation reading, so a stepped frame
moves both timelines exactly one period.
