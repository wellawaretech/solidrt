---
title: Android surface swap blocks four vsyncs
description: SOLVED, it was our 4x MSAA all along, the ~80 ms swap block was the GPU draining full off-tile multisample resolve traffic every frame. Fixed via a multisampled Android window backbuffer (in-tile resolve at swap) plus the rig's EXT_multisampled_render_to_texture path, whose resolve-out must be a sampling draw, never a blit (Adreno). TV at 50 fps / 4x MSAA / 0.1 percent drops; full investigation record and measurement rules inside.
created: 2026-07-28
completed: 2026-07-28
---

# Android surface swap blocks four vsyncs

## Resolved 2026-07-27: device-specific, not ours

Lead 1 below (reproduce on a second Android device) came back negative, which
settles it. Samsung SM-T500 (Adreno 610, Android 12, 60 Hz, arm64-v8a), same
runtime generation, same 20k-vertex trivial scene, measured at the same time:

| | tablet (Adreno 610) | TV (Mali-T860) |
|---|---|---|
| present interval | **16.7 ms, 59.9 fps** | 80 ms, 12.4 fps |
| `get_stats` fps / frameMs | 55 / 21.8 | 12 / 90.0 |
| `rasterQueue` | **1** | 3 |
| `fenceTimeouts` | 0 | 0 |

The tablet is vsync-locked at exactly one refresh period. So the engine's
present path is healthy on Android and reaches the display's clock when the
compositor cooperates - the four-to-five-period swap belongs to this 2017
MediaTek TV, not to alloy. Note `rasterQueue` 1 on the tablet, matching
desktop: the TV's persistent 3 was purely a symptom of the slow swap, not an
independent problem.

Reframed as a documented device limitation. Leads 2 and 4 below are no longer
worth speculative work; they would only matter if a second device ever
reproduces it. What is worth keeping is the app-facing consequence: on this
class of hardware the frame budget is set by the compositor rather than by the
GPU, so vertex-count tuning is close to pointless (see the table further
down - a near-empty scene and a 233k-vertex one differ by 40 ms against an
80 ms swap).

Split out of idle-tick-gpu-backlog-runaway.md, whose "present-fence pacing
caps throughput" adjacent finding turned out to be aimed at the wrong layer.
That item's candidates are all closed; this is the live remainder.

Device: Philips TPM171E (MediaTek MT5891, ARM Mali-T860, GLES 3.2 driver
r20p0, Android 8.0, armeabi-v7a, 1920x1080, **50 Hz** - `DispSync ... refresh
20000000 ns`). Runtime 0.0.38-22-g4c7e487-dirty, i.e. with two-deep present
fencing in place.

A near-empty animated scene - 20k point-topology vertices, trivial vertex
shader, one pass, no depth, one animated uniform per frame - presents once
every 80-100 ms (4-5 refresh periods), for **12.4 fps on a scene that should
be free**. The client asks for the right thing and the platform does not
deliver it.

Where the time goes, from the phase log on that scene:

| | fence wait | draw | present |
|---|---|---|---|
| before the fence work | 78 ms | 2 ms | 1 ms |
| with `PRESENT_FENCE_DEPTH = 2` | **0.0 ms** | 6 ms | **80-85 ms** |

The total did not move; only its owner did. `eglSwapBuffers` blocks for four
to five vsyncs on its own. Previously the depth-1 present fence absorbed that
same back-pressure *ahead* of the swap, which is why it read as fence pacing
and why raising the fence depth bought nothing here - the fence sat downstream
of the real constraint. `fenceTimeouts` is 0 throughout, so this is not a
pacing failure.

It is device-specific, not general. Identical binary, identical scene, live
`get_stats`:

| | fps | frameMs | rasterQueue | fenceTimeouts |
|---|---|---|---|---|
| desktop linux | 61 | 19.94 | 1 | 0 |
| Android TV | 12 | 89.97 | 3 | 0 |

The persistent `rasterQueue` of 3 against desktop's 1 is the same fact seen
from the other end: the raster thread stays ~3 commands deep on a trivial
scene purely because each swap takes four refresh periods to return.

## What this costs

It is a hard content-independent ceiling, so it sets the budget for every
GPU-heavy app on this class of device. Measured on the same TV, same session:

| scene | frame time |
|---|---|
| 20k vertices, trivial | 80 ms (the floor) |
| 34,800-vertex point-cloud flower | 100 ms |
| 233,600-vertex flower (GPU-bound) | 120 ms |

Note how little separates them. Going from a near-empty scene to 233k shaded
vertices costs 40 ms; the swap costs 80. Until this is understood, tuning app
vertex counts on this device is rearranging deck chairs - the third row is the
only one where the GPU is actually the limit.

## Leads, in order

1. **Reproduce on a second Android device.** A 2017 TV SoC deep-queueing its
   graphics plane is entirely plausible, and the 50 Hz panel is already
   unusual. If a modern phone shows a 1-vsync swap on the same binary this is
   a device quirk to document, not an engine bug to fix - and that is a
   30-minute check that gates all the work below. Do this first.
2. **Buffer queue depth.** Nothing in the tree configures the surface's
   buffer count; SDL creates the window and we take what we get. A swap that
   blocks exactly 4-5 vsyncs is the signature of the app being throttled to
   `queue_depth x vsync` with a deep queue. `dumpsys SurfaceFlinger` on the
   layer shows `queued-frames` 0-1 at steady state, so the queue is not
   visibly backing up - which points at a minimum-buffer / triple-plus
   buffering configuration rather than congestion. Worth reading what
   SurfaceFlinger reports for the layer's buffer slots on this device.
3. **Not swap interval.** `SDL_GL_SetSwapInterval(1)` is already called
   (alloy/src/raster.rs and alloy/src/gl.rs) and does not warn on this
   device, so the interval request succeeds and is not being honored in the
   way one refresh period would imply. Ruled out; recorded so nobody spends
   an afternoon on it.
4. **AChoreographer interaction.** The Android vsync backend (alloy/src/vsync.rs)
   defers `FrameRendered` to a choreographer callback plus a computed
   `PacingBudget` delay. If the swap already costs four periods, that delay is
   being added on top of a latency the pacing model does not know about.
   Check whether the emitted delay is contributing periods here before
   assuming all four are the compositor's.

## Not established

That all 4-5 periods belong to `eglSwapBuffers` rather than being split with
the choreographer-deferred frame release. The phase log attributes 80-85 ms to
the `present` span specifically, which is the swap call itself, so most of it
is - but lead 4 is untested, and the 80/100 ms alternation suggests something
is landing on a boundary rather than a clean multiple.

## Session 2026-07-28: TV-side settings ruled out; overlay path confirmed

Chasing the "deep display pipeline" theory from the TV side, over adb
(device: same Philips TPM171E, `adb -s 192.168.2.11:5555`; its settings menus
can be driven headlessly - `am start -n
org.droidtv.settings/.setupmenu.SetupMenuActivity`, then `input keyevent`
20/19/23/4 with `exec-out screencap -p` to verify each step).

**Picture processing is not the mechanism.**

- Motion styles (Picture > Advanced > Motion) was `Movie`; set to `Off`.
  Present cadence unchanged (233k flower: ~134 ms average before and after).
- There is no Game/PC picture mode for internal apps on this set - the
  option exists for HDMI sources only, per both the menus and Philips'
  support docs. Nothing else in the picture menu is a candidate.

**Two engine-free ways to measure the present rate**, both agreeing with the
phase-log numbers above:

- `dumpsys SurfaceFlinger --latency 'SurfaceView - ...'`: actual-present
  deltas quantized to 120/140 ms for the 233k flower.
- `screenrecord` + counting unique encoded frames (screenrecord only encodes
  on surface update): 36 frames in 5 s (~7.4 fps), in a bursty 56/213 ms
  alternation - two presents close together, then a long gap - which is the
  "landing on a boundary" artifact flagged under Not established.
- Control: the HWUI launcher presents frames 19-22 ms apart during focus
  animations (short bursts; leanback emits few frames per animation, so not
  proof of sustained 50 Hz, but 1-vsync presents do happen for HWUI layers).
- `wm size 960x540` moved the flower only 7.2 -> 9.3 fps: consistent with
  the 80 ms floor + content table above (and the flower's offscreen passes
  are fixed-size anyway).

**The layer is a hardware overlay.** `dumpsys SurfaceFlinger` HWC table:
`SurfaceView - com.solidrt.go... | Device` - the MTK overlay plane, and it
stays `Device` while screenrecord's virtual display is active, so every
measurement above is of the overlay path. Forcing GLES composition to test
the other path is blocked: `service call SurfaceFlinger 1008 i32 1` returns
"Operation not permitted" on this release-keys build, and `stop`/`start`
need root. Vendor ships `debug.sf.latch_unsignaled=1`.

**Lead 2 (buffer depth), sharpened.** The floor arithmetic fits exactly one
effective cycling buffer: cycle time = queue -> ~4-vsync release -> render
-> queue ≈ 80 ms, and 1/80 ms = 12.5 fps ≈ the measured 12.4. `queued-frames`
0-1 says the consumer is never backlogged, so the producer is starved in
`dequeueBuffer` waiting for a release - which more buffers would pipeline
over IF releases arrive as a FIFO (the 56/213 burst pattern is the main
counter-signal). The experiment: on Android, raise the surface buffer count -
`SDL_AndroidGetNativeWindow()` then the legacy `ANativeWindow::perform`
op `NATIVE_WINDOW_SET_BUFFER_COUNT` (= 2) to ~6 (armeabi-v7a, Android 8,
struct is stable AOSP), guarded to this device class. If throughput scales
with the count, ship it device-keyed; the added frames of latency are moot
on a device already ~3 frames deep (`rasterQueue` 3 x 134 ms ≈ 400 ms
input-to-glass).

**Adjacent latency item regardless of throughput:** with the swap this slow,
`rasterQueue` sits at 3, so interaction runs ~400 ms behind. Capping the
raster queue to 1 when swap-block is detected would cut perceived latency
~3x even if throughput stays 7-12 fps.

**Instrumentation lead:** `EGL_ANDROID_get_frame_timestamps` would attribute
the 80 ms definitively (queue->latch vs latch->release), but it is likely
absent on this r20p0 Mali driver (not in SF's EGL extension list; check the
client context's list before building anything on it).

**Below-Android escape hatch (untested):** the chassis exposes the MTK
driver CLI to the shell user - `/system/bin/cli_shell` + `/dev/cli`, output
via `logcat -s MTK_KL`. Confirmed present: `n.byp.pq <0|1>` (bypass all PQ)
and `vdp.s.game <id> <0|1>` (usage strings verified, not yet exercised;
state resets on reboot; see github.com/yath/tpm171e for the command tree).
If `n.byp.pq 1` collapses the present floor, that pins the 4 vsyncs on the
PQ pipeline specifically and makes a "TV dev mode" setup note worthwhile.

## Session 2026-07-28 (2): the block is queueBuffer; SF latches at ~6 Hz

Goal was fluent max fps on the TV. Probe app (projects/organism/src/
tv-probe.tsx, debug-switchable modes) established the ceiling is fully
content-independent: pure UI box 9 fps, static pipeline 8, tiny animated
pipeline 9, 1080p 20k-vertex pipeline 7 - same ceiling with no GPU pipeline
at all. The "no shaders = fluent" memory traces to the pre-3-thread engine,
not to content (see below).

Experiment matrix, all measured on-device (raster phase log + screenrecord
frame counting + atrace):

| change | result |
|---|---|
| Motion styles Off (TV menu) | no change |
| `cli_shell n.byp.pq 1` (bypass all TV PQ) | no change |
| swap interval 0 (WINDOW_SWAP_INTERVAL, kept) | no change (12 fps; window advertises min interval 0, so no clamp) |
| RGBA8888 EGL config (kept; SDL default was RGB565) | no change in rate (buffers confirmed 8888) |
| `Surface::setSwapInterval(0)` direct (async mode, kept) | no change |
| `perform(SET_BUFFER_COUNT, 5)` (kept) | 12 -> 14 fps; slots confirmed allocated |
| forced Client composition (pointer_location overlay) | no change |
| present-return pacing (vsync source disabled, reverted) | 14-15 fps, block moves entirely into swap |

atrace (`gfx sched binder_driver`) verdict: the block is **inside
queueBuffer, not dequeueBuffer** - eglSwapBuffersWithDamageKHR spans
alternate ~3ms / ~140ms and the 140ms is a sync-mode queueBuffer wait
(`waitForever`) for SurfaceFlinger to acquire the pending frame. Meanwhile
VSYNC-app ticks a clean 20.0ms (50 Hz healthy), SF wakes ~16/s, but
latchBuffer runs only ~6.4/s - most SF wakeups latch nothing for this
layer. The residual unknown is this MTK-modified SF build's latch policy
(vendor sets `debug.sf.latch_unsignaled=1`; suspicion: acquire-fence or
frame-pacing gating inside their fork). srt-raster blocks in
binder_thread_read 70% of samples (confirms the server-side wait).

Cross-check that the platform CAN do better: the pre-3-thread engine ran
the emoji/hero app fluently on this same TV (user report: "long ago -
before the refactor into 3 threads", i.e. before the ANGLE fix that
serialized GL onto one thread; the old loop touched the GL context from
two threads, worked on Linux/Android, broke on Windows/macOS under ANGLE).
So either the old frame discipline satisfied MTK SF's latch policy or the
regression is elsewhere in the refactor. **Next lead: check out or
reconstruct the pre-refactor present loop and measure it on this TV; diff
what it does differently at the window-surface level (thread that swaps,
buffers in flight at queue time, timing of queue relative to vsync, EGL
surface creation parameters).**

Two more negatives (2026-07-28, late): explicit per-frame buffer
timestamps via perform(NATIVE_WINDOW_SET_BUFFERS_TIMESTAMP) - testing a
"MTK SF rate-matches latches to observed timestamp cadence" theory - had
no effect (possibly stomped by Mali EGL re-setting AUTO at queue time;
helper left in sdl_utils.rs, unwired). And present-return pacing (vsync
source off) plus back-to-back production changed the block's shape only.
The latch-rate limiter behaves as if it sits below everything userspace
can reach without the pre-refactor comparison build.

## The A/B: emoji app, pre- vs post-refactor engine

The app the user remembers as fluent on this TV is sandbox/emojis. Ported
into projects/organism/src/emojis/ with the 24 Fluent SVGs embedded in the
bundle (src/emojis/svgs.ts, generated) because `file("./fluent-emoji/...")`
resolves client-side and fails under the dev server on both desktop and TV.

Measured on the CURRENT engine on the TV: **6 fps during scroll** (draw
14-16 ms - the scene is heavier than the probe - plus the same ~140 ms
queueBuffer block; screenrecord agrees at ~6.5 unique frames/s). The
reference app is therefore cleared of blame; the engine revision is the
variable, unless the TV itself changed state since the fluent era.

The refactor boundary in git: 8dff06d "GL serialization (fixes
ANGLE-dependend builds)" -> 61ccbfe "GL on single thread" -> 6b4797b
"Introduce raster thread", all 2026-07-19. Last pre-refactor commit:
**2ed4c8f** ("Alloy GL logging"). A worktree at
~/solidrt/tmp/pre-refactor builds it for armeabi-v7a (needs `bun install`
first for the bunx-srt default-app bundle step). Same applicationId, so
installing it replaces the go APK on the TV; reinstall the current build
afterwards.

## RESOLUTION 2026-07-28: the engine was never the variable

The A/B ran, twice:

- 2ed4c8f (2-thread-GL era, "v0.0.29"): connects to the current dev
  server, but current-core bundles fail (`srt:app` module missing) - no
  app measurement; not needed, because:
- **v0.0.22 exactly** (heroes' pinned version, worktree ~/solidrt/tmp/v0022,
  its own launcher animation as the workload): **~7.3 fps** - the same
  ceiling as today's engine. Fresh TV reboot: ~5.7 fps. TV firmware:
  unchanged since 2021-11-08 (no OTA between the "fluent era" and now).

So every engine revision ever measured on this TV presents at ~6-14 fps,
the firmware never changed, a reboot changes nothing, and every userspace
lever is exhausted. Conclusion: the Philips 49PUS7803's MTK SurfaceFlinger
latches external GL app layers at ~6-7 Hz as a standing platform behavior.
The remembered fluency ("emoji/heroes app, long ago") most likely belongs
to a different device (the SM-T500 tablet runs the identical scene
vsync-locked at 59.9 fps, desktop at 61) - no configuration has ever been
found in which this TV does better. Treat the TV as a ~12 fps target and
budget apps accordingly (the app-facing consequence tables above stand).

Cross-version compatibility notes from the attempt, for the record: a
0.0.22-era prebuilt bundle can be pushed by the current dev server (load
accepts .srt.js), needs `"imageWidth"/"imageHeight"` -> `"w"/"h"` patching
(commit 0a315e8) on newer runtimes, and still dies on the window-root
check against v0.0.29 native; era-correct serving works by bumping
DEV_PORT in the era CLI's dev-server.ts and pointing the APK at that port
via the srt_dev_server intent extra.

Hard-won API notes for whoever picks this up:
- `perform(NATIVE_WINDOW_SET_BUFFER_COUNT)` works pre-first-dequeue; the
  legacy ANativeWindow struct has lockBuffer_DEPRECATED between
  dequeueBuffer_DEPRECATED and queueBuffer_DEPRECATED - omitting it shifts
  query/perform onto queueBuffer and SIGSEGVs at
  Surface::getSlotFromBufferLocked with fault addr 0x3f (three crashes'
  worth of lesson).
- The go debug APK is debuggable: `run-as com.solidrt.go` + /proc wchan
  sampling works for "what syscall is this thread stuck in".
- MainActivity temporarily exports SRT_LOG=debug (marked TEMPORARY) so the
  raster phase log reaches logcat; remove when this closes.
- atrace categories on this TV: gfx sched binder_driver (no `sync`).

Related: adaptive-present-fence-depth.md (the fence work this sits behind),
idle-tick-gpu-backlog-runaway.md (parent item, with the full session data and
the remaining adjacent findings).

## REOPENED 2026-07-28 (evening): the resolution was built on a broken ruler

The user challenged the verdict ("a TV that plays video and runs Kodi
fluently cannot be a 12 fps device") and the challenge survives contact
with measurement. Three new facts, all from `dumpsys SurfaceFlinger
--latency` on-device (frame-latch timestamps out of SF's own ring buffer,
no capture path involved):

1. **Kodi's GLES UI runs at a rock-solid 50 fps on this TV.** Navigating
   Estuary menus, its `SurfaceView - org.xbmc.kodi/...Main#0` layer shows
   long runs of perfect 20.0 ms latch deltas, with ~500 ms idle gaps
   between animations (Kodi renders on demand). The TV launcher (HWUI)
   also bursts at exactly 20.0 ms during focus animations.
2. **screenrecord is not a valid fps instrument on this TV.** While SF
   latched Kodi at 50 Hz, `screenrecord` encoded 34 frames in 9.4 s
   (~3.6 fps); the launcher likewise. The virtual-display capture path is
   itself throttled to a few Hz on this MTK build. Every screenrecord
   number in the sections above - including the probe matrix and the
   entire v0.0.22 era A/B ("~7.3 fps, same as today") - is untrustworthy.
   The fluent-era memory can no longer be ruled out; the RESOLUTION
   section's verdict is withdrawn.
3. **Our engine's slowness is nevertheless real, and content-independent.**
   Same SF-latency method, same session: flower latches at ~240-260 ms
   (~4 fps); tv-probe mode 0 (single moving Box, no pipelines) latches in
   a 160 ms-dominated pattern (~7-9 fps effective) with occasional
   20 ms pairs. This is on the current binary, which still carries the
   interval-0 + async-mode + setBufferCount(5) experiments - themselves
   "validated" with screenrecord, so their value is now unknown.

Layer type is NOT the difference: both Kodi and our SDL app render into a
`SurfaceView` layer (SDL's SDLSurface extends SurfaceView). The compositor
demonstrably serves such a layer at 50 Hz for another GL app. The
difference is in how frames are produced/queued: Kodi = classic blocking
eglSwapBuffers, GL on one thread, demand-driven bursts; us = choreographer
-paced raster thread, PRESENT_FENCE_DEPTH=2, currently interval 0 + forced
async mode.

Instrumentation for the next round (in tree, marked TEMPORARY):
`SRT_SWAP_INTERVAL` env overrides the present path per launch, forwarded
from the `srt_swap_interval` intent extra by MainActivity
(`--es srt_swap_interval 1` = stock sync path, `0` = async experiment
path). See sdl_utils::window_swap_interval.

Measurement discipline from here on: SF `--latency` on the app's
SurfaceView layer only. No screenrecord. Engine fps stat free-runs and is
also not evidence.

### 2026-07-28 evening, round 2: the stall is downstream of SF composition

A/B via the new SRT_SWAP_INTERVAL toggle, tv-probe mode 0 (single moving
Box), SF --latency as the metric:
- interval 0 + async mode: 160 ms-dominated latch pattern, ~7-9 fps.
- interval 1 stock sync path: metronomic 20/80/160 ms cycle (3 frames per
  260 ms, ~11.5 fps). The async/buffer-count experiments were never the
  cause and barely matter; both paths hit the same wall.

Engine self-timing (SRT_LOG=debug): jsMs 0.5, layoutMs 0.1, draw 3-6 ms,
fence wait 0 - the entire frame cost is inside eglSwapBuffers
(43-148 ms). atrace (gfx sched binder_driver, 4 s) shows the full chain,
25 composition cycles in 4 s:

  srt-raster eglSwapBuffersWithDamageKHR 142.8 ms
    -> surfaceflinger handleMessageRefresh/doComposition ~145 ms
       -> postFramebuffer -> presentAndGetReleaseFences ~145 ms  <- HWC2
          present blocks here, inside the MTK vendor blob

Meanwhile SF's own compositor swap stat during Kodi animation: 12-16 ms
(vs 75-144 ms during our app). Kodi's SurfaceView runs two-buffer/double
-buffered 100% and latches 20.0 ms. So SF GLES-composites BOTH apps; the
HWC present is only slow when the content being presented is ours.

Two live theories, discriminated by the SRT_GL_FINISH probe (timed
glFinish between draw and present, in tree, TEMPORARY):
1. GPU-slow: with debug.sf.latch_unsignaled=1 SF latches our unfinished
   buffer, composites against our still-running GPU work, and HWC present
   waits on the FB acquire fence -> 145 ms IS our Mali frame time, even
   for a single box (stuck GPU governor? Impeller/Midgard pathology?).
   Kodi's GL is cheap enough not to care.
2. Display-driver-slow: our GPU is fine and the MTK fb consumer stalls
   present for our layer specifically (format/usage/cadence trigger).
   glFinish returning in a few ms while present still blocks would prove
   this branch.

### 2026-07-28 evening, round 3: MECHANISM FOUND - cadence-sensitive display
pipeline, and the fluent era was real

The SRT_GL_FINISH probe (timed glFinish between draw and present) settles
it. tv-probe mode 0, both present paths:

  fence wait 0.0ms, draw ~2ms, finish 80.0-80.4ms, present ~1ms   (sync)
  fence wait 0.0ms, draw ~2ms, finish ~77ms,       present ~1ms   (async)

A Mali-T860 does not need a flat, metronomic 80.0 ms (exactly 4 vsyncs)
to draw one box at 1080p. The GPU job is not slow - it sits waiting on
the backbuffer's release fence. The display pipeline holds each buffer
~4 vsyncs after present WHEN THE PRESENT CADENCE IS SLOW; at a steady
50 Hz feed it retires promptly (Kodi runs 50 fps two-buffered - impossible
under an unconditional 4-vsync hold). The state is self-sustaining in
both directions:

  fast: queue always fed -> present every vsync -> prompt retire -> no wait
  slow: any cadence gap -> pipeline holds 4 vsyncs -> next frame waits
        80 ms -> bigger gap -> locked at ~12 fps

This finally explains the history: the PRE-REFACTOR engine was a classic
blocking-swap continuous loop (Kodi's pattern - queue always fed), so the
TV stayed in the fast state: the remembered fluent emoji/heroes sessions
on this TV were REAL. The 3-thread refactor made production vsync-gated
(choreographer waits before each frame), which on this display is exactly
the pattern that locks slow. Every content-level and buffer-level lever
failed because none of them changed the production cadence.

Engine direction (next round): a "feed the pipe" mode for cadence-
sensitive displays - do not gate frame production on the choreographer
tick + present-fence depth; produce render-ahead so the BufferQueue always
holds a pending frame (blocking swap as pacer, like desktop), possibly
with a startup burst to flip into the fast state. Desktop already paces
by blocking swap, so this is Android-specific plumbing, not a redesign.
Open question: how many consecutive on-cadence presents flip the state
(Kodi needs its queue pre-fed by only ~2 frames, suggesting few).

Instrumentation left in tree (all marked TEMPORARY): SRT_SWAP_INTERVAL
(srt_swap_interval extra), SRT_GL_FINISH (srt_gl_finish extra),
SRT_LOG=debug in MainActivity. Measurement rule stands: SF --latency
only; screenrecord and the engine fps stat both lie on this TV.

## SOLVED 2026-07-28: it was the MSAA resolve, all along

SRT_MSAA=0 (single-sample window rig) on the completely stock present
path - no feed-pipe, no fence-gate bypass, no buffer-count raise, no
layer-config changes - gives a rock-solid 20.0 ms latch cadence (50 fps)
on tv-probe mode 0, and the glFinish probe collapses from a flat ~80 ms
to 5.8 ms. Human-confirmed fluent on the panel ("FLUENT!" via the probe's
remote-feedback path). The flower goes ~8 -> ~17-18 fps with a measured
53 ms/frame of genuine content GPU work remaining (points + glow pass) -
a normal optimization target now, not a platform mystery.

The real mechanism: Impeller's GLES window path renders into a 4x
multisampled renderbuffer and blit-resolves to FBO 0 every frame. That is
full off-tile multisample traffic - ~1920x1080 x 4 B x 4 samples of color
store plus depth/stencil plus the resolve read+write, roughly 200 MB of
DDR traffic per frame - which on this TV's memory bus is ~80 ms, flat,
regardless of content. Every observation in this file follows from that
one number:
- the ~80 ms "release fence wait" was the GPU draining its own resolve;
- the content-independent ~12 fps ceiling (80 ms + scene cost);
- SF's compositor swaps inflating to 75-145 ms only while our app runs
  (its GLES composite queued behind our 80 ms of Mali work);
- the 20/80/160 latch patterns (queue-shape quantization of an 80 ms
  producer, reshaped but never fixed by the queue experiments);
- Kodi/launcher/HWUI being fluent (nobody else multisamples);
- the tablet being fine (Mali-G52's bandwidth and a smaller relative
  cost) and desktop being fine (discrete bandwidth);
- and the pre-refactor engine having been fluent IF that era's window
  path was single-sample (unverified, but the refactor-era rig is when
  4x MSAA became unconditional).

The cadence-sensitive-display theory in the round-3 section above is
WITHDRAWN - the display was innocent; everything it "held" was
backpressure from a pegged GPU.

Fix directions, in order of value:
1. EXT_multisampled_render_to_texture (in this TV's extension list, and
   standard on tiled GPUs): in-tile resolve makes 4x MSAA nearly free -
   the proper fix that keeps AA. Requires the impellers GLES backend to
   use it for the window rig.
2. Until then: scale MSAA to the device (SRT_MSAA plumbing is in tree;
   the old device-perf-model backlog item is the natural home - weak
   tiled GPU => samples=0/2).
3. App-level: the flower's remaining 53 ms is points + glow passes;
   ordinary content optimization from here.

Verdict on the experiment toggles this hunt produced: feed-pipe,
no-fence-gate, buffer-count raise, translucent/alpha/on-top, hwui-pulse
all proved unnecessary for the fix (the queue experiments reshaped
patterns but could not beat an 80 ms GPU floor). They are all env-gated
and marked EXPERIMENT/TEMPORARY; candidates for removal once SRT_MSAA
handling is productized. SRT_GL_FINISH and SRT_LOG forwarding earned
their keep as diagnosis tools.

Measurement rules that made this solvable (keep for posterity):
- dumpsys SurfaceFlinger --latency on the app's SurfaceView layer is the
  only trustworthy external fps meter on this TV; screenrecord caps at
  ~3.5 fps and the engine fps stat free-runs.
- A timed glFinish between draw and present separates GPU execution from
  present blocking; the flat 80 ms it exposed was the whole answer.

### Final fix, shipped 2026-07-28: multisampled window backbuffer on Android

Two implementations of the in-tile idea were measured, SF --latency,
hiccups counted as latch gaps >= 39 ms:

1. EXT_multisampled_render_to_texture in the rig (render into a single-
   sample texture with implicit in-tile resolve, then blit to FBO 0):
   50 fps but 4-9% dropped frames at 4x (draw 10-13 ms, spikes past the
   20 ms budget - the extra fullscreen copy plus 4x tile overhead), 2-5%
   at 2x. Human-visible micro-stutter ("not 100% fluent").
2. Multisampled window backbuffer (Android requests EGL samples on the
   window config; plain frames wrap FBO 0 directly - no rig pass, no
   copy; the driver resolves in-tile at swap): 4x MSAA,
   **0 hiccups in 250 frames**, perfect 20.0 ms cadence. Flower: steady
   40/60 ms alternation (~20 fps), all remaining cost content-genuine.

Landed state: Android window = multisampled backbuffer (SRT_MSAA
overrides the sample count, 0/1 = single-sample); plain window frames
skip the rig; the rig keeps the EXT in-tile path for the window-layer
(shader) target and falls back to the explicit resolve where the
extension is missing; desktop unchanged (single-sample window + rig,
bandwidth does not care). read_fbo0_pixels resolves through a temp FBO
when the backbuffer is multisampled (glReadPixels cannot read MSAA).

The swap-latency experiment code (feed-pipe, fence-gate bypass, buffer-
count raise, translucent/alpha/on-top, hwui-pulse) is removed; SRT_MSAA,
SRT_GL_FINISH, SRT_SWAP_INTERVAL, and srt_log forwarding remain as
diagnosis levers.

### Addendum: the last hiccups were CPU preemption, fixed with thread priority

With the multisampled window landed, ~1-3 latch gaps of 40 ms per 125
frames remained in every config (including single-sample - and Kodi's
own 50 fps stream shows the same rate). logcat over 12 s: cast_shell
(Chromecast service) runs V8 GCs every few seconds and a vendor service
polls a Hue bridge over HTTP - the little cores are shared and a
default-priority frame thread loses ~20 ms about once a second.

Fix: srt-raster now runs at Android display priority -8 and the UI
thread at -4 (what HWUI's own threads use; children inherit it, so the
JS/tokio workers land at -4 too). Census after: 750 steady-state frames,
2 hiccups (0.27%, ~1 per 7.5 s) - the residue is other processes' own
noise, at parity with the TV's native apps.

### Post-fix pruning (user direction)

All investigation toggles are gone: SRT_MSAA, SRT_SWAP_INTERVAL and the
SRT_GL_FINISH probe (recipe stays in this doc), the ANativeWindow legacy-
ABI module with the async-mode/timestamp experiments, and every srt_*
intent-extra forward in MainActivity (including srt_log) - the Android
app template is back to pristine. What ships: the 8-bit color request,
the multisampled Android window backbuffer (4x, in-tile resolve at swap;
plain frames wrap FBO 0 directly), the EXT_multisampled_render_to_texture
path for the rig's window-layer target with explicit-resolve fallback,
the multisample-aware read_fbo0_pixels, and frame_thread_priority() -
now cross-platform (Android/Linux setpriority -8/-4, macOS QoS classes,
Windows SetThreadPriority; best-effort, debug-logged when denied).

Final acceptance on the TV, plain launch, no flags: 1 dropped frame in
1001 (0.1%) at a 20.0 ms latch cadence with full 4x MSAA.

### Adreno follow-up 2026-07-28: the rig's in-tile resolve must sample, not blit

The shipped rig EXT path broke the Samsung SM-T500 (Adreno 610, Android
12): a continuous `GL error 0x502 after shader pass` stream, the screen
flashing between the last two stale swapchain frames, and persistent
black screens on static content (the failed frame is never re-requested
under demand-driven rendering). Two facts made it device-wide there: the
SM-T500 is DENIED a multisampled EGL backbuffer config (startup now
always logs `window backbuffer is ...` either way), so unlike the TV,
every plain frame runs the rig EXT path - and Adreno rejects blitting
out of that rig. Two escalating failure modes:

1. As shipped, the resolve blit read from the MSRTT-attached draw FBO.
   Such an FBO answers GL sample queries as multisampled, which makes it
   an illegal blit source: GL_INVALID_OPERATION on every ext frame.
2. Blitting from a second, plain FBO wrapping ext.color (the resolved
   texture) fixed steady frames but still threw 0x502 content-dependently
   - first repro was frames introducing new glyphs (Impeller glyph-atlas
   maintenance mid-draw), so it survives light testing: the launcher
   home ran clean for 13 s before the first menu killed it.

The fix is the extension's canonical consumption path: no blit at all.
The resolve-out is a fullscreen sampling draw of ext.color into the
destination (`EXT_RESOLVE_COPY_SRC` + `shader::render_program_to_fbo`,
program cached on `OffscreenRig.copy`, freed on the ext-unavailable
latch; the fragment maps the window rect out of the 64-px-aligned
allocation via textureSize, exact at pixel centers). Bandwidth-identical
to the blit: one full-frame read plus one write. Verified on the SM-T500
same day: launcher, detail cards, organism, recurse all clean, zero GL
errors.

TRAP: never "simplify" this back to glBlitFramebuffer - failure mode 2
is content-dependent and passes a quick look. And when a latched-error
stream appears, attribute it by draining glGetError per stage
(temporarily); the reporting site of a latched check is not the source.

TV check, same day: the sampling-resolve build ran on the TV looking
unchanged (visual check, not an SF --latency census) - as expected, since
the TV's plain frames use the multisampled backbuffer and skip the rig
entirely; only window-layer/shader frames touch the changed code. The
SM-T500, always on the rig EXT path, is the standing regression canary
for it.
