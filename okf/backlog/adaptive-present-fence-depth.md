---
type: backlog-item
title: Adaptive present-fence depth
description: Fallback design if unconditional two-deep present fencing ever shows up as desktop drag latency - allow the second in-flight frame only when observed fence waits show the GPU is over budget.
status: deferred
timestamp: 2026-07-27T00:00:00Z
---

# Adaptive present-fence depth

`PRESENT_FENCE_DEPTH` in `alloy/src/raster.rs` is 2, unconditionally, since
2026-07-27. The measurement that justified it (see
idle-tick-gpu-backlog-runaway.md): on a saturated 50 Hz TV the depth-1 fence
expired at its 100 ms timeout on every presented frame - ~100 ms of the
~140 ms frame period was capped wait providing no pacing at all - while on
desktop `fenceTimeouts` reads exactly 0 and fence waits are single-digit
milliseconds.

The trade unconditional depth 2 accepts: on fast-GPU desktops the CPU may
run one frame further ahead of glass, which is input-to-photon latency in
exactly the interaction the frame-pacing work (okf/plans/frame-pacing.md)
optimized. The 1000 Hz-mouse drag persona is who would notice. Decision
2026-07-27: ship unconditional, revisit only if this is actually observed.

If it is observed, the fallback is to key the second slot on evidence of an
over-budget GPU rather than granting it always:

- Gate: allow depth 2 only while recent fence waits run long (e.g. the
  previous wait exceeded half a refresh period, or a timeout occurred within
  the last N frames); collapse back to depth 1 when waits return to
  milliseconds. Hysteresis matters - flapping between depths every frame
  would reintroduce pacing jitter.
- The signals already exist: `fenceTimeouts` (cumulative, in get_stats) and
  the per-fence wait time measured in `frame()` (`wait_ms`, already fed to
  FrameTiming).
- Verification: desktop drag latency back to depth-1 behavior (drag examples
  under the instrumented flow of okf/plans/frame-pacing.md), TV throughput
  keeps the depth-2 gain, `fenceTimeouts` still climbing on the TV under
  saturation (waits are capped either way there).

Caveat (2026-08-04): on ANGLE/D3D11 `glClientWaitSync` never blocks, so the
per-fence wait duration this gate keys on is always ~0 there - the gate
signal would need the instant-expired count or a `GetSynciv` spin instead.
See [angle-present-fence-pacing](angle-present-fence-pacing.md).
