---
title: The Android video plane against the compositor
description: What putting fullscreen video on its own SurfaceView under a translucent UI showed on a Philips TV (Android 8) and a Galaxy Tab A7 (Android 12) - HWC composition with a translucent SDL layer, plane handles across an engine reload, 25-on-60 pulldown and the vsync snap, SurfaceFlinger's one-comparison latch rule, why an isolated UI present costs the video a frame, and the fence wait that fixes it at a measured cost to animating apps.
created: 2026-09-12
---

# The Android video plane against the compositor

Cut from [android-video-punch-through](../done/android-video-punch-through.md)
at its close; each bullet is dated by the run it came out of. The census
recipe is in that record's "Verification, on device"; the vsync release chain
of an ordinary app on the same tablet is
[android-vsync-release-chain](android-vsync-release-chain.md).

## First runs

- 2026-09-12, first run on the TV (Philips TPM171E, `clip1080_silent.mp4`,
  1080p25, the plane page in examples/video): the VIDEO layer's
  SurfaceFlinger census over the whole 30 s clip was 127 presents with 126
  intervals of exactly two refresh periods and no singles or triples - the
  "done looks like" cadence on the first run, with no runtime tuning. The
  same after seeks and a pause. The UI layer presented about three times a
  second (the overlay readout) while the video ran: video holds no frame
  demand at all.
- The translucency checkpoint passes: with the SDL layer flagged
  translucent (isOpaque=0, format RGBA_8888) both SurfaceViews are
  DEVICE-composited by the HWC, video beneath (z=-2) and UI above (z=-1).
  No GPU (client) composition, so nothing regresses for apps that play no
  video, and the runtime-flip fallback is not needed.
- The window backbuffer was already cleared to transparent black on the
  Android fast path; the black seen before was the layer's opaque flag, not
  the pixels. Stage 1 therefore has no raster change at all.
- The codec that opened in surface mode is the same hardware one buffer mode
  used (`OMX.MTK.VIDEO.DECODER.VP9`, 1920x1088 vendor buffers, 17 dequeued
  buffers). ACodec logs `setPortMode ... DynamicANWBuffer failed` and
  `nBufferCountActual` warnings on this vendor OMX and falls back
  internally; harmless.
- Transport on the plane, measured through the debug commands: seek on a
  finished stream restarts it from the target; pause holds the position
  exactly (the surface keeps the latched frame); seek while paused reports
  the exact target pts (the frame is released once); resume continues from
  there at 1x. The `toggle` debug command reports the pre-command state
  because commands cross a channel; a debug artefact, not a player one.

## Plane handles across an engine reload

- 2026-09-12, tablet (Samsung SM-T500, Android 12): the same page played
  invisibly - position advanced at 1x on `OMX.qcom.video.decoder.vp9`, the
  overlay drew, the screen stayed black, and SurfaceFlinger listed no layer
  for the plane. The activity's view tree showed the plane view gone within
  3 s of the open. Cause: the Java handle was a singleton ("remove the
  current plane"), and on a reload the OLD engine tears down after the NEW
  one has opened its plane, so the old plane's posted removal took the new
  view. The codec kept decoding into the detached BLAST surface (frames are
  consumed in-process, nothing reaches the compositor), and at end of stream
  Samsung's MediaCodec released the codec itself when the dead surface
  failed to disconnect - the "codec shutdown at EOS" that first looked like
  a worker bug. The TV happened to order the two the other way. Fix:
  `createVideoPlane` returns the view and `destroyVideoPlane(view)` removes
  that view by identity; alloy holds it as a JNI global ref. General lesson
  for JNI handles crossing an engine reload: never address platform
  objects as "the current one".

## The tablet: composition and cadence

- 2026-09-12, tablet after the identity fix: the picture shows, letterboxed
  (the plane view measured 1941x1092 in the 2000x1092 landscape window),
  colours correct, overlay over it. The plane's buffers are native Venus
  NV12 (fourcc `NV12`, format 0x7FA30C04) and the per-frame
  `C2D: Start color convertion` lines the detached surface had caused are
  gone. HWC difference from the TV: with a plane present the qcom HWC
  composes the translucent UI layer as CLIENT (GPU) and the video as
  DEVICE; without a plane the UI layer was DEVICE. So on this device the
  translucency cost exists only while a plane is open, when SurfaceFlinger
  already runs per video frame anyway. The TV keeps both on DEVICE.
- Tablet cadence, 25 fps on a 60 Hz panel: every frame is delivered (127
  presents in ~4.8 s) but the census is 32 single-period, 50 two-period, 20
  three-period and 24 longer intervals, where the ideal on 60 Hz is an
  alternation of two and three periods only (40 ms is 2.4 vsyncs; no
  hardware compensates that, it is the 24p-on-60Hz pulldown every panel
  shows, and the Tab A7 panel has no 50 Hz mode for the frame-rate API to
  pick). The singles and the 4+ intervals are jitter of one vsync either
  way on top of that, and they are ours to remove: a `desiredPresentTime`
  that lands just past a vsync's latch deadline slips a whole period, which
  is why ExoPlayer's VideoFrameReleaseHelper snaps every release time onto
  the vsync grid (the closest vsync, minus an offset). The TV is immune
  (Android 8, no BLAST, and 40 ms is exactly two of its periods). Proposed
  next step for the plane: snap release times to the vsync grid from a
  Choreographer-sampled phase and the known period. Not built yet.
- 2026-09-12, tablet, vsync snap built (the release schedule in the plan's Transport section).
  Baseline re-taken first under the same conditions: 127 presents in
  5.07 s, 30 single, 51 two, 22 three, 23 longer intervals. With the snap
  and the 50 ms lead unchanged: 126 presents, 29 / 52 / 21 / 23, no
  change in the census - but the latency dump shows the snap working:
  every desired present time sits 0.7 periods before an actual present
  (the baseline's were spread uniformly over the period), and 80 of 126
  frames present exactly on their snapped vsync. The other 46 present one
  (20), two (20) or three (3) vsyncs after it, in a lateness sequence of
  "2 1 0 0 0" repeating every five frames, i.e. once per 12-vsync cycle of
  the 25-on-60 pulldown: deterministic, not jitter. The desired times
  are right; the frames reach SurfaceFlinger late. The tablet's phase
  offsets are 1 ms app and 1 ms SF, so a buffer has to be at SurfaceFlinger
  before its wake one period ahead of the vsync, and this SurfaceView is a
  BLAST queue (client side, Android 12) that forwards one pending buffer
  and holds the next until a release comes back: a frame handed over
  three periods early waits behind the hold and its transaction lands after
  the wake it needed. The TV's Android 8 queue lives inside SurfaceFlinger
  and holds several buffers, which is why the 50 ms lead was harmless
  there. Fix: the lead becomes one refresh period when the grid is known
  (`RELEASE_LEAD_PERIODS`), the 50 ms constant staying as the fallback.

## Drawing over the plane: what costs the video a frame

The first reading below (every UI present costs one video frame) is revised
by the last entry of this section: what costs a frame is an ISOLATED present.
The Choreographer phase bug found on the way was fixed by
[choreographer-vsync-phase-offset](../done/choreographer-vsync-phase-offset.md).

- 2026-09-12, the release-to-latch path, and the cause: on Android 12 every
  present on the UI layer above the plane costs one video frame its vsync,
  one for one. Measured on the tablet with the census, all other things
  equal (fullscreen, two layers, both DEVICE, SurfaceFlinger composing
  25 times a second exactly as it does for the stock player):

  | UI layer presents | video frames slipped | intervals off the 2/3 pattern |
  |---|---|---|
  | 5 a second (the example readout) | 5 a second, 20% | 40% |
  | 1 a second (the dev badge alone) | 1 a second, 4% | 6% |
  | stock Samsung player, same clip | 3% | 6% |

  Exactly linear, and with the readout hidden our census equals the stock
  player's (4 single, 70 two, 48 three, 4 longer against its 5/69/49/3).
  Nothing else moved the number. A sweep inside one run over vsync offsets
  of 20, 50 and 80 percent and release leads of one, three and six periods
  stayed between 57 and 65 percent on pattern, and our request stream is
  provably exact over 700 frames: pts steps all 40 ms, requested intervals
  only two and three periods in a 59/41 split, and the request phase on the
  grid exact to three decimals (`(due - vsync) mod period` = 0.800, 0.500,
  0.200 for the three offsets, zero scatter). The Choreographer sample is at
  most 1.11 periods old when it is used. So the anchor, the snap, the offset
  and the hand-over lead are all correct and none of them is the defect.
  Going fullscreen (`<window fullscreen>`; a windowed client has the status
  bar, the navigation bar, Samsung's edge-panel handle and the activity's
  own window in the stack, six layers with five on GPU) removed all client
  composition and improved the census from 53 to 47 bad intervals out of
  125, so GPU composition was a contributor and not the cause.

  The TV does not have this: its UI layer presented at 5 a second and then
  1 a second during a capture in which the video layer was 126 of 126
  intervals at exactly two periods. Android 8 has no BLAST and its buffer
  queue lives inside SurfaceFlinger, which is the difference. So this is a
  BLAST-era interaction between two layers of the same app, not something
  about the clock.

  Consequence for the design: an app that draws over fullscreen video on
  Android 12 pays one dropped video slot per overlay repaint. A dev client
  always pays about one a second for the connection badge, which is where
  the residual 6% comes from and is the same residual the stock player has.

- 2026-09-12, the floor, measured: a PACKED app (`srt pack --apk` on
  `examples/video/src/probe/plane_clean.tsx`, which renders `<window
  fullscreen />` over the plane and presents nothing after startup, so there
  is no dev badge either) gives 76 two-period and 48 three-period intervals
  out of 126, with one single and one seven from a startup disturbance. That
  is 98% on the ideal 25-on-60 pattern, and the 76/48 split is the exact
  60/40 the arithmetic calls for. It beats the stock Samsung player's 94% on
  the same clip and panel. So the plane path reaches the theoretical maximum
  for 25 fps on a fixed 60 Hz panel, and everything left is the cost of
  drawing over it.

  Full ladder on the tablet, 25 fps, intervals in refresh periods:

  | | 1 | 2 | 3 | 4+ | on pattern |
  |---|---|---|---|---|---|
  | packed, nothing drawn over the video | 1 | 76 | 48 | 1 | 98% |
  | dev client, badge only (1 present/s) | 4 | 70 | 48 | 4 | 94% |
  | stock Samsung player | 5 | 69 | 49 | 3 | 94% |
  | dev client, readout on (5 presents/s) | 27 | 50 | 25 | 24 | 60% |
  | windowed, readout on, 5 layers on GPU | 30 | 52 | 17 | 26 | 55% |

- 2026-09-12, what the mechanism is NOT. From an aligned systrace (gfx, view
  and binder_driver) plus the census and the per-frame probe, all in one
  window, with the overlay on: SurfaceFlinger wakes about 48 times a second
  (mostly every vsync), wakes 15 ms before the vsync it targets, spends
  0.8 ms on a cycle that latches nothing, 5.6 ms on one that latches the
  video and 9.4 ms on one that latches both layers, and NEVER finishes after
  the vsync it aimed at, in any of 191 cycles. So SurfaceFlinger misses no
  deadline and composition cost is not the cause. The buffer reaches it
  promptly too: from our `releaseOutputBufferAtTime` to the BLAST queue
  forwarding it is 0.05 ms at the median and 0.22 ms at the worst, with no
  difference between frames near a UI present and frames away from one, so
  there is no app-side scheduling or binder contention either. Every slipped
  frame is late by whole periods and none is ever early. Correlating present
  intervals against UI presents inside the same trace: intervals containing
  a UI present are 54% on pattern (n=22) against 87% for intervals without
  one (n=71), and 10 of the 11 long intervals in the capture contain a UI
  present. Half the UI presents share a vsync with a video frame and are
  harmless; the ones that land on their own vsync are what costs a frame.

- 2026-09-12, the Android 12 latch rule, read from AOSP (frameworks/native
  android12-release and lineage-19.0), with the numbers this display forces:

  - `releaseOutputBufferAtTime`'s timestamp becomes a SurfaceControl
    transaction `desiredPresentTime`, not a buffer timestamp anyone
    re-interprets. It gets there through two async looper hops inside
    MediaCodec and ACodec plus a one-way binder call, so the release call is
    not synchronous.
  - The whole policy is ONE comparison in
    `SurfaceFlinger::transactionIsReadyToBeApplied`: a transaction is
    withheld unless `desiredPresentTime < mExpectedPresentTime` (strictly;
    equal is NOT ready), with a one-second escape hatch. There is no second
    timing check at latch: `BufferStateLayer::isBufferDue` returns true
    unconditionally. SurfaceFlinger emits `not current` in the trace each
    time it withholds, which is how the counts below were taken.
  - ARRIVAL DEADLINE: to be composed for target vsync T the transaction must
    reach SurfaceFlinger strictly before T - 15.667 ms, which is 0.94 of a
    period, i.e. within the first millisecond after the previous vsync. That
    figure is forced by the dumpsys line: `presDeadline` 16666666 with a
    16666666 period gives an SF phase of +1 ms, hence an SF work duration of
    15.667 ms, and the wake is scheduled at the first predicted vsync after
    now plus that. Our hand-over at 1.8 periods before T clears it by
    14.3 ms, and arriving earlier is harmless because a withheld transaction
    sits in `mPendingTransactionQueues` and every non-ready flush re-signals
    invalidate, so it is re-examined at each following wake. That is why the
    lead sweep changed nothing.
  - A slightly past-due request is latched at once; there is no "too old"
    rule. But SurfaceFlinger will silently DROP the older of two buffers of
    one layer that become ready in the same flush (`BufferStateLayer::
    setBuffer` replaces the pending buffer), and a non-auto timestamp
    defeats the backpressure guard that would otherwise prevent it. Dropped
    frames are the likely reading of the four- and five-period intervals:
    they are missing rows in the `--latency` dump, and the requested-spacing
    histogram shows the same four- and five-period gaps.
  - Client (GPU) composition of any layer forces `canSkipValidate` false, so
    SurfaceFlinger cannot take the fused presentOrValidate path and must
    validate, run a full-screen GPU pass and present, every frame the video
    updates; it also drops the present-fence grace time to zero. That is the
    windowed case and why fullscreen helped.

  OUR BUG, found by the same read and confirmed by our own numbers: the
  Choreographer grid is one millisecond late in phase. `frameTimeNanos` is
  the app's target WAKEUP time, not the vsync; the app is woken
  appWorkDuration + sfWorkDuration = 32.333 ms = 1.94 periods before its
  vsync, and 32.333 mod 16.667 = 15.667, so our grid points sit 1.0 ms AFTER
  the true vsyncs. Our requests therefore land at V - 0.74 of a period
  rather than the intended V - 0.80, which is exactly the 0.7 the census
  measured and which I had read as scatter. The correction is
  `trueVsync = frameTimeNanos + (period - Display.getAppVsyncOffsetNanos())`
  modulo the period. It does not cross the gate at any offset we tested, so
  it is a correctness fix rather than the cadence fix.

  And `setFrameRate` is a dead end on this tablet by construction, not by
  policy: `RefreshRateConfigs::canSwitch` requires more than one mode, so
  LayerHistory is inert and content-based rate selection returns
  immediately; frame-rate override needs a pairwise divider of at least two
  between modes, so it is unavailable with one mode, and
  `getFrameRateDivider(60, 25)` is zero regardless. Nothing about the latch
  policy, the phase offsets or expectedPresentTime changes. It stays worth
  calling for devices that do have modes, which is the TV's case rather than
  this one.

- 2026-09-12, THE LAW IS NOT WHAT IT LOOKED LIKE, and the pacing fix is
  refuted. Measured on a pinned build with the sweep probe removed, same
  clip, same fullscreen client, only the overlay's repaint rate changed:

  | overlay presents | 1 | 2 | 3 | 4+ | on pattern |
  |---|---|---|---|---|---|
  | off (dev badge only, about 1 a second) | 1 | 73 | 51 | 1 | 98% |
  | 5 a second (the 200 ms readout) | 25 | 59 | 17 | 25 | 60% |
  | every frame requested, 25 a second delivered | 3 | 76 | 41 | 6 | 93% |

  Presenting MORE often is dramatically BETTER, so "each present costs a
  video frame" is wrong. What costs a frame is an ISOLATED present. At one a
  second there is about one victim a second; at five a second there are five;
  at twenty-five none, because no present is isolated and the app's own layer
  self-paces to the compositor's rate through back-pressure without anything
  being built to make it.

  This kills the proposed fix of pacing overlay presents into the video's
  compositor cycle. That fix would deliberately put our buffer in the commit
  that carries the video frame, and the reason an isolated present costs a
  frame is that its buffer arrives with a fence that has not signalled: the
  systrace shows our render fence exceeding a refresh period 16 times out of
  16 (median 49.3 ms, range 27 to 76), SurfaceFlinger's slow composite fences
  all ending 4 to 9 ms after one of ours, and the kernel's
  `plane_wait_input_fence` at p90 25.5 ms and max 58 ms. A DRM atomic commit
  is per-CRTC, so one plane's slow in-fence holds the video plane's flip with
  it, and the next commit blocks behind the stuck one (12 of 13 successor
  cycles slipped). Pacing would move that latency from a commit the video is
  not in to the one it is in.

  And it is NOT our own render cost: the runtime's own figures for the
  overlay frame are 0.65 ms at p50, 1.3 at p95 and 1.8 ms worst over 100
  frames, with 150 kpx of damage. Sub-millisecond work with a 49 ms fence is
  wake latency on a GPU that a trickle of tiny frames leaves in its deepest
  idle state, which is also why twenty-five a second is fine and five a
  second is not.

## The fence wait

The adaptive refinement named at the end is
[plane-adaptive-fence-wait](../backlog/plane-adaptive-fence-wait.md).

- 2026-09-12, FIXED, and it is exact. The window's buffer must not carry
  unfinished GPU work into the commit it shares with the plane: while a plane
  exists, the raster thread waits on a fence for the frame's own GPU work
  before the swap (`RasterFrame::finish_gpu_work`, bounded by the same
  PRESENT_FENCE_TIMEOUT_NS as the pacing wait). The wait is then spent on our
  thread, the window frame lands a commit later, and the video keeps its slot.

  | overlay at 5 presents a second | 1 | 2 | 3 | 4+ | on pattern |
  |---|---|---|---|---|---|
  | before | 25 | 59 | 17 | 25 | 60% |
  | after, three consecutive runs | 0 | 76 | 50 | 0 | 100% |
  | | 0 | 76 | 50 | 0 | 100% |
  | | 0 | 75 | 50 | 0 | 100% |

  Not one anomalous interval in 377, and 76 twos to 50 threes is the exact
  60/40 the arithmetic demands. It beats every other configuration measured,
  including drawing nothing over the video at all (98%) and holding
  continuous frame demand (93%), so the two alternatives are moot. The app
  pays nothing measurable for it: its own frames stay at 0.63 ms p50 and
  0.99 ms worst with no slow frames and no fence timeouts, because the wait
  costs only what the GPU still owes on a sub-millisecond frame. The cost is
  a commit of presentation latency on the UI, taken deliberately: while a
  video is on screen its cadence is worth more than a frame of UI latency.

  Gated on a plane existing rather than on it playing, so a paused plane pays
  the wait for nothing. That is a refinement rather than a defect.

  What it costs an app that wants more than the video's rate, measured with
  the overlay's ticker at every vsync so the app requests about 60 frames a
  second: the video still holds (tablet 125 of 126 and then 126 of 126
  intervals on pattern, TV 121 of 125), and the app is capped near the
  video's own rate, 17 to 20 a second on the tablet and 26 on the TV. Before
  the fence the same demand delivered 25 a second on the tablet at 93%. So
  the wait costs an app about a third of its throughput at high demand and
  buys the video the last seven percent. The cause is serialization: draw,
  wait for the GPU, then a swap that blocks for the rest of the period, so a
  frame cannot overlap its own GPU work with the next frame's build. That is
  the right trade for an overlay over fullscreen video and the wrong one for
  an app that wants to animate freely over it; if that case turns up, the
  refinement is to skip the wait while the app is already presenting every
  vsync, since a warm GPU signals its fence promptly and there is nothing to
  protect against.

  2026-09-12, and this is the case that decides whether the fence wait can
  stay as it is: a real transition running continuously over the plane
  (`examples/video/src/probe/plane_transition.tsx`, a marker sliding on the
  position lane, retargeted every 2 s). The video is perfect, 76 twos and 50
  threes twice over, and the animation is unwatchable at 16 frames a second.
  The control (`transition_only.tsx`, the same transition with no video at
  all) runs at 51 a second on both devices with a 0.11 ms build, so our
  drawing is not the problem and nothing about the animation path is:
  everything degrades only while a plane is open.

  | over a plane | video on pattern | app fps |
  |---|---|---|
  | transition, fence wait on | 100% | 16 |
  | continuous demand, no fence wait | 93% | 25 |
  | overlay 5 a second, fence wait on | 100% | 5 |
  | overlay 5 a second, no fence wait | 60% | 5 |
  | transition, no plane at all | n/a | 51 |

  So a plane caps the app at about 25 a second on its own, and the fence wait
  takes that to 16. Both numbers are the cost of sharing one atomic commit
  with the plane. The fixed policy is therefore right for a static overlay
  and wrong for an animating one, and the resolution that follows from the
  measured law is to make the wait adaptive: an isolated present is what
  costs the video a frame, so wait only when the previous present was more
  than about a period ago and skip it while the app is presenting every
  frame. That would give a static overlay its 100% and an animating app its
  25 a second at 93%, instead of forcing one choice on both. Not built.

  Unrelated and worth not confusing with any of this: with no video at all
  the tablet runs the transition at 51 of its 60 frames, which is the loop's
  own long-standing ceiling on this device (the PacingBudget comment in
  `alloy/src/vsync.rs` records the same ~5 skipped frames a second and what
  causes them). The TV's 51 IS its maximum, since its panel is 50 Hz.

  The TV needed it too, which the first run's luck had hidden. Re-measured
  before installing the fix, with its badge presenting once a second, the
  Philips gave 6 single, 113 two and 7 three-period intervals, 90% on
  pattern; 25 fps into its 50 Hz panel is exactly two periods, so every
  single and every three is pure error with no cadence group to blame. With
  the fence wait, and its overlay presenting five times a second, three
  consecutive runs gave 126, 126 and 125 intervals of exactly two periods and
  nothing else. So the fix is not tablet-specific and not Android 12
  specific: sharing a commit with the plane is what costs the frame, and
  Android 8 shares it too.

## Colours

- 2026-09-12, TV colours on the plane checked by eye: correct. The MediaTek
  colour risk named under verification did not show, so both devices meet
  the colour criterion.
