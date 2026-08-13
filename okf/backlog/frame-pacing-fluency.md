---
type: backlog-item
title: Frame pacing - motion on the TV is not fluent
description: RESOLVED 2026-08-13. Not a regression; ~1.4 percent clustered latch drops on all builds came from the vsync-release chain. Fixed by the FramePacing policy switch - SwapPaced (present-return pacing) for touchless devices, VsyncLocked kept for touch - measured 0.00-0.04 percent drops on the TV. Direction-1 hardening (generation tags, request-anchored fallback, slewed PacingBudget) kept for phase-lock correctness. Also fixed: the InputDevices touch fact now gates on Android's touchscreen feature (SDL over-reports on TV boxes).
status: resolved
timestamp: 2026-08-13T00:00:00Z
---

# Frame pacing - motion on the TV is not fluent

## Session 2026-08-13: measured, bisected, mechanism found

Probe: examples/spin/src/pacing.tsx - three bars sweeping at constant
velocity (4/2/1 s per screen width), ~5 nodes, no textures, no GPU passes.
Constant-velocity motion is the only content the eye judges fluency by;
the old spin example (1 rev/100 s) can never show stutter. Metric:
`dumpsys SurfaceFlinger --latency` on the app layer, 90 s censuses
(the ring holds 127 frames; sample every 2 s and dedup by timestamp).

### The A/B table (all measured same day, same TV, same probe)

| build | date | drops (>=39 ms) |
|---|---|---|
| 5551d84 MSAA-acceptance era | 07-28 | 1.43% |
| 39ca207 pre lattice->alloy moves | 08-10 | 1.26% |
| f3910dd main | 08-12 | 1.26-1.67% |
| feature-video tip (dirty) | 08-13 | 0.8-4% (short windows) |

**No regression. The regression hypothesis (PacedClock split / frame-driver
move / surface-liveness move / resampler move) is disproven by direct A/B.**
The 07-28 acceptance record ("1 dropped frame in 1001") was a lucky 20 s
window: drops cluster (gaps between drops ranged 0.04 s to 11.5 s in one
90 s run), so clean 20 s stretches exist in every session.

Also ruled out, each by direct measurement:
- Dev server connection (same rate with server killed, probe standalone).
- TV uptime/background state (same rate after fresh reboot).
- Thread priorities (verified live: srt-raster -8, srt-vsync/SDLThread/
  tokio -4 - the MSAA-era priority fix is intact and inherited).
- The 1/s refresh-rate safety-net query (drop spacing is not periodic).
- The platform itself: the TV launcher under the same key-driven
  measurement latches essentially perfect 20.0 ms today.

### Mechanism (from SF queue timestamps; no engine instrumentation)

SF's --latency column 1 is queueBuffer time, so queue-to-queue deltas show
production cadence directly. Two regimes, visible in a single dump:

- Phase-locked: queue deltas flat 19.7-20.3 ms. In this state buffer
  margins (ready-to-latch, col2 - col3) sit at a flat ~48 ms - the
  BufferQueue is saturated, the blocking swap is the real pacer, and
  presentation is metronomic (this is the desktop pacing model emerging).
- Oscillation: queue deltas alternate around the mean (8/32, 12/27,
  16/25 ms pairs) for seconds at a time. Late release then catch-up: the
  choreographer grid holds, so one late release costs +j and the next
  reads -j. The queue absorbs the wobble (presents stay 20.0 ms) until a
  swing exceeds the remaining slack -> one missed latch (40 ms hold),
  catch-up burst, repeat.

The release chain on Android is: choreographer callback (srt-vsync)
-> sleep(delay from PacingBudget) -> SDL push -> main loop drains ->
FrameRendered -> JS exec -> build -> raster -> queueBuffer. Measured from
JS (probe's debug command): frame callbacks arrive in bursts - pairs ~1 ms
apart then 35-43 ms gaps, sd 8-9 ms, ~11 percent of gaps over 30 ms -
while the panel latches flat 20.0 ms. Per-release jitter up to ~15 ms
enters between the choreographer and the queueBuffer. Known amplifiers in
alloy/src/app.rs + vsync.rs:

- The fallback releases at pending_since + 2 periods, guaranteeing a
  ~40 ms production gap whenever a signal is late/lost.
- A stale signal (one that arrives after its present was fallback-
  released) is only discarded if nothing is pending; if the next present
  is already pending it releases it early -> the 1 ms double-release
  pairs that keep the oscillation alive.
- PacingBudget's delay is worst-of-32 emission-to-present + 2 ms, so one
  slipped frame shifts the release phase for a whole window.

The engine's own oddity confirming lost beats: ~1.1 idle Ticks/s fire
during continuous animation (frameStep 0 events in JS), which should
never happen while every frame latches a new request.

Note the irony: the engine is at its most fluent exactly when the queue
saturates and it degenerates to blocking-swap pacing - the state the
vsync phase-lock was designed to avoid (it optimizes input-to-glass
latency, which a remote-control TV does not care about).

### Paced clock: cleared

The animation timeline the app reads is smoothed by PacedClock to
sd ~0.6 ms even under the bursty release delivery (tick deltas 18.3-23.1,
GAIN correction as designed). ~0.6 ms at these speeds is ~1 px - not the
visible stutter. Do not redesign the clock for this.

### Fix directions (design discussion pending - propose before editing)

1. Robustness inside the phase-lock (incremental): generation-tag vsync
   requests so stale signals are always discarded; shorten the fallback
   (2 periods -> ~1.25) so a late signal costs 5 ms not 20; consider
   clamping PacingBudget's phase swings. Keeps the low-latency state,
   should cut the drop rate; does not remove the oscillation mechanism.
2. Fluency-first pacing policy for non-touch/TV-class devices: produce
   one frame ahead so the BufferQueue stays fed and the blocking swap
   paces production (the desktop model; the clean regime already IS
   this). Costs ~1 frame of input latency - irrelevant on a TV, wrong
   for the tablet, so this wants a policy switch (input modality or
   device class), which needs a design decision.
3. Real presentation timestamps (PresentClock KNOWN ISSUE): Android
   EGL_ANDROID_get_frame_timestamps / choreographer timestamps would let
   pacing measure truth instead of modeling it. Bigger lift; unverified
   driver support on this TV (likely absent on Mali r20p0).

### Measurement discipline (carried forward from the MSAA saga, extended)

- ONLY SF --latency on the app's SurfaceView layer. Engine fps stat and
  screenrecord lie on this TV.
- 90 s+ censuses; drops cluster, so 20 s windows can read 0 or 4 percent.
- Control against the TV launcher's layer (key-driven focus animations)
  to separate engine trouble from platform/environment trouble.
- atrace on this TV: -t mode with -b 8192 worked once; buffer resize can
  OOM afterwards, and --async_stop can wedge unkillably in the kernel at
  100 percent CPU (only a reboot clears it) - and its load poisons every
  concurrent measurement. Prefer SF timestamps; they carry queue time
  (col 1), fence-ready time (col 3), and latch time (col 2), which
  answered everything atrace would have.
- The dev-pushed probe auto-installs as a store app ("Spin"), so it can
  run without the dev server: kill the server after load and the app
  keeps running - useful for isolating dev-server influence.
- Cross-version A/B: a current-CLI bundle loads fine into a runtime a
  few days old, but the 0.0.38-era runtime BSODs on a current-core
  bundle - run the era's own dev server from its worktree instead
  (worktree + bun install + its bunx srt server; port is the same fixed
  34884, so kill the current server first).

## Session 2026-08-13, part 2: direction 1 implemented, direction 2 proven

Direction 1 (phase-lock hardening) was implemented in alloy (uncommitted):
- Generation-tagged vsync requests (vsync.rs): each request supersedes the
  previous; try_take discards signals from superseded requests, so a stale
  signal can never release the next present early. The fallback path now
  disarms + re-requests, superseding the late signal.
- Request-anchored fallback (app.rs): the deadline is request time + period
  + armed delay + 4ms slack (the latest a healthy signal can arrive), not
  present-return + 2 periods. A lost signal now costs a ~1.6-period
  production gap instead of 2-3.
- Slew-limited PacingBudget delay (0.5ms/frame max movement): worst-of-32
  steps become phase drifts instead of phase jumps.

Measured result: NO improvement. Five 90s censuses across the two hardened
iterations: 2.04/1.52/1.79 then 1.72/1.44 percent - the baseline band.
Key insight from margins: the hardened build runs the queue SHALLOWER
(~39ms mean ready-to-latch slack vs ~48ms in the clean regime), because
the old stale-signal early releases were an accidental produce-ahead that
topped up the queue. Correct phase-locking reduces exactly the slack that
absorbs platform jitter. The amplifiers were real but were not the cause;
the cause is phase-locked production having ~1 frame less queue slack than
this platform's jitter needs.

GC ruled out (user question): a debug-toggleable garbage generator in the
probe ("garbage" command). 1500 objects/frame (75k/s churn, ~1000x the
probe's baseline allocation) -> 2.15 percent, indistinguishable from
control. (10000/frame saturates the armv7 CPU outright - 80-100ms frames -
that measures CPU cost, not GC pauses; keep amplification under the frame
budget.)

Direction 2 proven decisively: a throwaway build with the vsync source
disabled (present-return pacing, the desktop model - queue fills, blocking
swap paces production):
- novsync-1: 4568 intervals, mean 20.00ms, 0 drops
- novsync-2: 4564 intervals, mean 20.00ms, 0 drops
- JS side: frameStep flat 1 (sd 0.041, zero lost beats), tick sd 0.33ms,
  while wall-clock callback jitter stays sd ~8ms - the queue absorbs all
  of it.
Zero drops in 180s, cleaner than the TV launcher control. The entire drop
mechanism lives in the vsync release chain; swap-paced production is
perfectly fluent on this device.

## Session 2026-08-13, part 3: pacing policy implemented - RESOLVED

The policy switch shipped the same day:
- alloy: `FramePacing { VsyncLocked, SwapPaced }` (vsync.rs) applied by the
  main loop via `AlloyCommand::SetFramePacing`. VsyncLocked is the default
  and the previous behavior; SwapPaced emits the frame signal at
  present-return (the desktop model - queue fills, blocking swap paces).
  Switching to SwapPaced releases any vsync-deferred presents immediately;
  the superseded vsync signal drains harmlessly (generation tags).
- lattice (policy, above alloy per layering): the InputDevices arm of the
  event loop derives the policy from the touch fact - touch present ->
  VsyncLocked (finger-to-glass drag latency), no touch -> SwapPaced - and
  re-sends on hotplug. Logged at info: "input devices: ... -> pacing ...".
- Fact fix en route: SDL's touch enumeration LIES on this TV (reports a
  touch device; Android declares no android.hardware.touchscreen feature
  and no input device carries a touchscreen source). InputDevices' touch
  fact is now gated on PackageManager.hasSystemFeature via
  sdl_utils::has_touchscreen_feature() (JNI, cached, fails toward true).
  Without this the policy read touch=true and never engaged - and JS
  design-system policies would see a phantom touchscreen too.

Measured on the TV (same probe + census discipline): policy-1 0.04%
(2 drops in 90s, right after launch), policy-2 0.00%. Matches the
experiment. Baseline was 1.26-1.67%.

Notes:
- The fact-derived default is accepted implicit magic for now; making
  pacing (and its siblings) app-readable and app-overridable is tracked
  in [[runtime-policy-registry]].
- flux's direct-render GUI path does not send SetFramePacing and stays
  VsyncLocked; wire it up if fluency matters there (dev tooling today).
- The direction-1 hardening stays: it makes VsyncLocked correct (no
  early releases from stale signals, 1.6-period bounded fallback, smooth
  pacing-delay drift), which touch devices still use.
- Desktop is untouched: no vsync backend, both policies behave the same.

## Older context (pre-session, kept for history)

Symptom as filed: on the Philips TV (MT5891, 50 Hz panel) a synthetic
360p moving-bar clip visibly stuttered at both 50 and 25 fps
(examples/video/src/probe/pacing.tsx, feature-video branch), and the
user reported previously-fluent motion no longer reading fluent. Engine
stats suggested a ~27 ms base frame-loop latency, but engine stats are
untrusted on this TV. The video-decode side of that investigation lives
in [[texture-upload-staging]]; the 1080p case remains raster-bound there.
The open MSAA-path question from the original filing is answered by this
session implicitly: plain frames present at a clean 20 ms cadence, so
the multisampled-backbuffer fast path is intact.
