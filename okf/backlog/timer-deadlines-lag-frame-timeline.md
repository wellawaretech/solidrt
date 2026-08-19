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
