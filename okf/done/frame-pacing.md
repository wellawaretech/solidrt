---
title: Frame pacing
description: "Cut Android touch-drag latency and the 60/60 input-versus-frame stutter: fire the frame signal from an AChoreographer vsync callback, then late submit, then input resampling."
created: 2026-07-25
completed: 2026-07-25
---

# Frame pacing

Cut Android touch-drag latency and stutter by driving frame production from
the display's vsync clock instead of from present-return, then sampling input
against that clock. Desktop is explicitly fine today (per-event input, swap
blocks on vsync); this plan is Android-scoped behavior on a shared scheduling
seam.

## Problem, as measured (tablet, 2026-07-25)

Instrumented drag sessions (examples/drag, release client; the 1/s
`[alloy] input:` and `[alloy] raster:` log lines added for this work):

- Steady 60 fps, draw ~1ms, present ~1.5ms (never blocking), fence wait
  8-10ms. The scene costs nothing; every frame is on time.
- Yet the tiles trail the finger by roughly 4-6 frames. The latency is
  pipeline structure, not throughput:
  1. Android batches touch to its vsync before the app sees it (~1-2 frames).
  2. A move that lands just after a paint waits for the next frame signal
     (0-1 frame).
  3. We paint immediately after present-return, so the submitted buffer sits
     in the BufferQueue until the compositor latches it (~1-2 frames). Present
     never blocks (queue never full), which is how "submit as early as
     possible" maximizes queue-sitting time.
- Single-finger drag also shows a beat stutter: Android's input batching
  (60Hz, vsync-locked) and our frame clock (present-return, not locked) drift
  in phase, so some frames find no fresh move (repaint same position) and the
  next double-steps. At 120Hz unbuffered delivery the beat mostly disappears
  (verified: 120 moves/s with the SDLActivity patch, see below).

## What already landed (2026-07-25, this investigation)

- Present fence (alloy/src/raster.rs): a GL fence after each swap, awaited
  before the next draw, capping CPU-ahead-of-GPU to one frame. Did not cut
  drag latency here (frames are GPU-trivial, the fence signals early) but is
  the correct backstop for GPU-bound scenes. All platforms. Keep.
- Per-pointer move dispatch gate (lattice/src/runtime.rs `pending_moves`):
  at most one move dispatch in flight per pointer; arrivals overwrite a
  latest-position slot the queued closure consumes. Fixes input overload:
  two-finger drag (240 moves/s) had saturated the JS thread (fps 44, stats
  queries timing out, seconds of stale moves replaying after finger-up).
  Verified fixed on device. This also bounds the Windows 1000Hz-mouse cost
  recorded in local-pointer-coords.md ("Related: move hit-test cost");
  winbox verification still pending.
- SDLActivity `requestUnbufferedDispatch` patch: tried, verified working
  (120 moves/s vs 60), REVERTED. It halves input staleness and masks the
  beat, but it forks vendored upstream Java and this plan makes it
  unnecessary (phase lock kills the beat; resampling makes delivery rate
  moot). Re-add only if the prediction extension ever wants fresher samples;
  see the SDL bump checklist note.

## Stages

### Stage 1: begin-at-vsync (AChoreographer)

Fire the frame signal from an `AChoreographer` vsync callback instead of
present-return. This is Flutter's model. Two wins: frame production
phase-locks to the same clock Android batches input on (kills the beat
stutter at any delivery rate), and the frame consumes the input batch that
arrives at a fixed phase before it.

Sketch: a looper thread (AChoreographer needs an ALooper) requests a
one-shot vsync callback only while a frame is wanted, posting the frame
signal into the existing alloy event channel. One-shot re-arm fits the
demand-driven contract exactly: no request, no callback, no wakeups when
idle. Non-Android keeps the present-return path; the seam is where the
frame signal originates, nothing downstream changes.

Implemented 2026-07-25 (device verification pending):
- `alloy/src/vsync.rs`: platform-neutral `VsyncSource` compiled everywhere;
  `start()` returns `Some` only where a backend exists (Android's
  choreographer thread today; an iOS backend would slot in via
  SDL_SetiOSAnimationCallback). Sole consumer of the new android-only deps
  `ndk` (safe looper) + `ndk-sys` (three raw AChoreographer calls; no safe
  binding crate covers them, checked 2026-07-25). Both deps drop if SDL
  ships its own choreographer API (libsdl-org/SDL#15013, milestone 3.8.0,
  no PR yet; we are on SDL 3.4.10).
- `alloy/src/app.rs`: on `FrameOutput::Presented` with a live vsync source,
  the FrameRendered defers until the vsync signal (fps counting stays at
  present time); one signal flushes all pending. Idle Ticks are suppressed
  and the event wait retargeted while a present is pending. Fallback: if no
  signal within two refresh periods, emit anyway (warn) - covers a silent
  choreographer, a dead vsync thread, and pause/resume races. A null
  choreographer or broken looper degrades to answering requests
  immediately, i.e. present-return pacing.
- Deliberately not yet fed through: the callback's frameTimeNanos (the
  signal is emitted sub-ms later on the main loop; the paced clock models
  time as frame counts). Revisit in stage 2, whose deadline math wants the
  real vsync timestamp.

Constraints:
- Demand gate and DL reuse must behave identically; only signal timing moves.
- Playback mode is lockstep (one FrameRendered in flight) and must bypass
  pacing entirely.
- PacedClock: choreographer callbacks carry the real vsync timestamp; feed it
  (better than the smoothed estimate), but keep the clock's model unchanged.
- Refresh-rate changes: the callback timestamps track the display; the
  existing DisplayRefreshRate event stays the authority for tick_period.

### Stage 2: deadline offset (late submit)

Refine stage 1 by starting the frame late in the vsync period rather than at
its start: begin at (next_vsync - budget), budget = draw cost + GPU cost +
margin, estimated from the raster timing stats. This is Chrome's
BeginFrame-with-deadline flavor; it trims most of the remaining queue-sitting
time and makes the input sampled as fresh as possible. Only worth doing once
stage 1's numbers are in; the budget estimator is the risky part (a missed
deadline costs a whole frame; start conservative).

Implemented 2026-07-26 (device verification pending), reshaped by stage 1
verification findings:
- Stage 1 device runs exposed two scheduling races the plan had not
  predicted: (1) a signal at the vsync itself races the vsync's own input
  batch crossing from the platform thread into the SDL queue and loses often
  (37-44 frames per 60 moves) - hence a signal delay is mandatory, not just
  a freshness optimization; (2) re-arming the choreographer at
  present-return puts the whole build pipeline ahead of the re-arm and
  misses the next vsync (fixed by pre-arming at signal emission,
  `vsync_armed` in app.rs).
- `PacingBudget` (alloy/src/vsync.rs): rolling 32-sample window of
  emission-to-present pipeline cost measured directly in the main loop
  (signal_emitted mark closed by the matching Presented; Tick-triggered
  presents not sampled). delay = period - (window max + 2ms margin), clamped
  to [min(period/2, 8ms) input-arrival floor, period - margin]; samples
  > 1.5 periods are slips and excluded. Empty window = floor (= the interim
  fixed half-period this replaces).
- `[alloy] pacing:` 1/s log line reports the current delay; `[lattice] js:`
  1/s line (lattice/src/runtime.rs JsTiming) reports move-dispatch and
  frame-closure avg/max on the JS thread, input-gated. The js line is the
  measure-first item: if the pipeline costs ~10ms on the tablet, the
  estimator sits at the floor until JS dispatch gets cheaper, and the max
  column separates steady cost from GC spikes.
- Device-verified 2026-07-26. Verdict on the remaining ~5 skipped frames/s
  (raster 50-56 vs 60 signals): NOT cost, NOT arrival tail. The js line
  showed dispatched moves at 53-58/s against 60 arriving with the gate only
  coalescing near-simultaneous arrivals, i.e. the platform delivers nothing
  at one vsync and a pair at the next ~5x/s. A floor experiment (10ms vs
  8ms) changed nothing, confirming pairing over jitter; 10ms also eats
  build margin (window max ~7ms + 10 brushes the period), so the floor went
  back to min(period * 0.6, 8ms). Empty-vsync frames correctly skip under
  demand semantics; the visual is a 1-frame stall + double-step ~5x/s -
  exactly what stage 3 resampling dissolves (an interpolated position makes
  every drag frame dirty regardless of delivery pairing).

### Stage 3: input resampling (and the smoothness endgame)

Sample pointer positions at frame time from a short per-pointer history
(position + timestamp pairs), interpolating to a fixed sample point
(Flutter resamples to now - ~5ms). Replaces "latest arrival wins" with a
continuous signal: smooth at any delivery rate, batched or not, and the
final word on the beat stutter. Extension, not baseline: extrapolation /
prediction (Android 13+ has MotionPredictor; linear extrapolation is the
cheap version) to visually cancel part of the latency floor.

Constraints carried over from local-pointer-coords.md ("Related" section):
- Idle hover must keep working with no frame in flight: when no frame is
  scheduled, moves keep dispatching on arrival (the gate already bounds
  them); resampling applies only when a frame consumes input.
- down/up/wheel stay on arrival, ungated and unsampled (ordering and deltas).

Implemented 2026-07-26 (device verification pending), reshaped by two
findings:
- Timestamp interpolation (the Flutter baseline) is off the table: SDL's
  Android path stamps touch at JNI receipt (SDLSurface.java passes no
  getEventTime(), SDL_android.c sends timestamp 0 = now; upstream main
  identical, checked 2026-07-26) and drops historical batch samples, so a
  paired delivery arrives as two events with near-identical timestamps -
  exactly the degenerate input. Real times would need forking the vendored
  Java plus a side JNI channel (onNativeTouch's signature is fixed in the
  registry crate's bundled C).
- Timestamps were not the crux anyway: at the empty vsync the missing sample
  physically has not arrived, so interpolation-only either holds (stall
  stays) or samples > 1 period in the past (re-adds the frame of latency
  stages 1-2 removed). The zero-latency fix is bridging the gap with one
  extrapolation step - "extension" material in the draft, actually
  load-bearing.
- Design landed (alloy/src/resample.rs, slot model): vsync batching makes
  samples nominally one per frame signal and a pair's two samples really one
  period apart, so each frame() call is a slot and no clock is needed. Per
  touch pointer per slot: fresh sample -> dispatch newest (a pair after a
  bridged gap lands as one normal step); first empty slot -> dispatch
  latest + last step (the frame that would stall); second consecutive empty
  slot after a bridge -> dispatch the real latest once (settle; the bounce
  is at most the one bridged step, only on abrupt stops), then silent.
- Wiring (runtime.rs): touch moves feed the history instead of dispatching
  on arrival; frame() drains sample() at signal time and dispatches ahead of
  the frame work in the same exec closure (JsTiming still splits move vs
  frame cost). Down seeds the history (first move has velocity), up clears
  and stays on-arrival, engine swap clears like the gate. Mouse and pen keep
  the arrival path untouched (hover constraint holds by construction).
  lib.rs batch coalescing exempts touch (a pair's older sample carries the
  velocity); mouse coalescing stays for the 1000Hz case. Idle Ticks keep
  frame() at refresh cadence, so buffered moves dispatch within a period
  even when nothing paints; non-Android touch degrades to latest-wins
  (per-event delivery rarely leaves an empty slot). Unit tests in
  lattice/src/tests/resample.rs cover steady/pair/stop/multi-pointer.
- Device-verified 2026-07-26 (tablet, release): steady single-finger drag
  went from raster 50-56/60 with moves 53-58 to raster 57-60 with js moves
  58-60 ~= frames - every frame consumes a move, the pairing gaps are
  bridged and render. Two-finger: input 120-122/s, js moves 116-124 (= 2
  per frame), raster steady 60, move cost 0.2ms - no saturation, no
  post-lift replay. User confirms the still+double-step beat is gone and
  the abrupt-stop settle bounce is not noticeable. The full-timestamp route
  stays available as the prediction extension's foundation if ever needed.

### Measure first: per-move dispatch cost

RESOLVED 2026-07-26 by the `[lattice] js:` instrumentation: move dispatch
averages 0.3ms (max ~1-3ms), frame closures 0.4ms, on the tablet in
release. The old ~4ms/move estimate from two-finger saturation math was
measuring exec-queue wait under overload, not execution cost; the GC/
allocation-churn theory is dead (no spike pattern). Dispatch cost is a
non-issue for pacing and for stage 3's calculus.

## Verification

- The `[alloy] input:` / `[alloy] raster:` 1/s log lines (alloy/src/app.rs,
  raster.rs) plus get_stats fps/frameMs are the instruments; they were added
  for this investigation and stay.
- Tablet protocol: single-finger drag (smoothness: no still+double-step
  frames; latency: visibly tighter than present-return build), two-finger
  drag (fps stays ~60, stats queries answer mid-drag, nothing replays after
  lift), idle (zero choreographer wakeups: callback only re-armed on demand).
- Windows: confirm the 1000Hz mouse behaves under the dispatch gate (winbox
  flow), closing the item recorded in local-pointer-coords.md. VERIFIED
  2026-07-26: behaves correctly. The 1/s diagnostic lines were demoted to
  debug level after verification (info-level noise served its purpose);
  raise to debug logging to read them again.

## Non-goals

- Desktop pacing (Wayland presentation feedback, DXGI waitable swapchains):
  recorded as possible futures; nothing today needs them.
- Reapplying the SDLActivity unbuffered-dispatch patch as a baseline.
- Touch prediction beyond the stage 3 extension note.
