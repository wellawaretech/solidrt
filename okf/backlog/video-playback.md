---
title: Video playback
description: One decode-to-YUV pipeline on every platform (software decoders on desktop, MediaCodec buffer mode on Android), planar YUV textures + shader conversion in alloy, player core in forge, no video primitive - texture/d-texture display the player's texture id. Fluency target is the Philips MT5891 TV; punch-through rejected. Probed 2026-08-12: the MTK decoder emits honest NV12 in buffer mode at 3x realtime for 1080p; the AImageReader fallback tap is unsupported on the device (not needed).
created: 2026-08-12
---

# Video playback

Designed 2026-08-12 in discussion; decisions below are settled, the module
internals wait on one on-device probe.

Build gate (2026-08-16): the whole stack is opt-in behind the `video` cargo
feature (forge owns it, flux and lattice pass through; `video-timeline-pacing`
implies it). A default build carries no decoder, no `flux:video` module, no
`video` capability, and `@solidrt/core` exposes no `/video` subpath. Enable
with `--features video` while the work is incomplete.

## Goal and scope

Video playback as a SolidRT capability, fluent on ALL devices including the
weakest connected target: the Philips TPM171E Android TV. First format
scope: H.264 + AAC in MP4 (the dominant baseline). Breadth later; the
pipeline shape is codec-agnostic.

## Target device facts (probed via adb, read-only, 2026-08-12)

Philips TPM171E, MediaTek MT5891, Android 8.0, API 26 (exactly the minimum
for the native AImageReader/AHardwareBuffer path), ARMv7 4-core, Mali-T860.
Hardware decoders (media_codecs.xml): AVC, HEVC, VP9, VP8, MPEG2, MJPEG +
a DSP full of audio codecs (AAC, AC3, DTS, ...). UI composits at 50 fps
after the MSAA fix, see [[android-surface-swap-latency]].

## Pipeline decision: one pipeline everywhere, buffer mode

Decode produces timestamped planar YUV frames as plain CPU bytes; alloy
uploads the planes as textures and converts YUV to RGB in a fragment
shader. Identical on every platform - software decoders on desktop and
MediaCodec on Android are just two producers of the same YUV frames.
Hardware decode is what makes the TV fluent; the upload is ~3 MB/frame at
1080p YUV420 (vs 8 MB RGBA), well inside the TV's budget on the Kodi
precedent below.

Rejected and settled:

- Surface punch-through (decoder output composited by SurfaceFlinger under
  the UI, Kodi's "MediaCodec (Surface)" mode): NOT an option (user
  decision). Breaks video-as-texture composability, Android-only special
  path against the single-rendering-path principle, needs JNI + meddling
  with SDL's Android view.
- Zero-copy surface import (AImageReader -> AHardwareBuffer -> EGLImage ->
  external-OES texture): not built now. If measurement ever demands it, it
  is a per-player OPT-IN rung, never the default, and lands in alloy (it is
  GL). It forfeits the postprocessing tiers below - external-OES samplers
  are invisible to Impeller and app shaders.

Field evidence for buffer-mode viability on exactly this device class:
Kodi's non-surface "MediaCodec" toggle (hardware decode into its GL YUV
renderer) was the documented working configuration on these TVs (the FAQ's
"video will still be hardware decoded" path, required for its GL scalers),
while the documented Philips bugs (pink/purple color corruption) were in
SURFACE mode, and ExoPlayer carries MTK-specific surface workarounds
(codecNeedsSetOutputSurfaceWorkaround). On this platform the buffer path
was the reliable one.

- https://kodi.wiki/view/Archive:Android_FAQ
- https://forum.kodi.tv/showthread.php?tid=353073
- https://github.com/google/ExoPlayer/issues/3445

## Layering

No new crate edges: alloy does not depend on forge and still will not.

- `forge::video` - the capability core, engine-free (YUV planes as plain
  bytes, no GL/SDL types). Demux via symphonia (all platforms; AAC decode
  too, pure Rust, cheap on CPU). Video decoder trait with two impls first:
  openh264 software decoder (PoC and dev fallback, see the decoder
  decision below), and AMediaCodec via the ndk crate under cfg(android)
  (platform-specific code in forge has precedent: subprocess, p2p). Player logic lives here: play/pause/seek state, decode worker
  thread, frame queue, and clock-agnostic sync - `advance(clock_pos) ->
  Option<YuvFrame>`; the caller feeds the master clock in.
- alloy - a video-agnostic texture-system feature: planar YUV textures in
  the TextureRegistry plus a YUV-to-RGB conversion pass in the raster
  path, with color metadata (BT.601/709 matrix, limited/full range) as
  uniforms. Camera could later use the same path and drop its CPU
  conversion. Rendertree untouched.
- flux `alloy_plugins/video.rs` - thin marshal + a per-tick hook (camera
  tick precedent): read the audio clock, call the forge player's
  advance(), hand the due frame's planes to alloy's upload. Sync decisions
  stay in forge, upload mechanics in alloy.
- `@solidrt/core/video` - openVideo() -> player handle (texture id, dims,
  duration, currentTime, play/pause/seek, close). NO video primitive
  (user decision 2026-08-12): `<texture>`/`<d-texture>` are the display
  targets (d-* first for animation-frequency content), matching the
  no-image-primitive texture-id currency. The player exposes its handles
  so a richer Video component composes in a higher layer, not core.

A/V sync: the audio position is the master clock; video frames are
dropped/duplicated against it by the forge player. Video PCM goes out
through a NEW streaming sink in alloy audio built directly on an SDL3
audio stream (SDL3 mixes all bound streams natively), deliberately NOT
through SDL3_mixer - this sink doubles as the pilot for the later mixer
replacement (staging item 6).

## Postprocessing tiers

1. Free by construction: the converted video texture is an ordinary
   texture id - custom fragment shaders, offscreen targets, and subtree
   effects apply unchanged.
2. Designed-for extension: expose the Y/UV plane textures as registry ids
   plus the color-metadata uniforms, so an app shader fuses conversion
   with grading/tone-mapping/LUTs in a single pass. This is why the plane
   textures live in the registry rather than as raster-internal scratch.
   Build when a consumer exists.
3. (Sugar over 2, unplanned): the built-in conversion pass accepting a
   replacement fragment shader.

## PROBED 2026-08-12: buffer mode emits honest NV12 at 3x realtime

The risk was proprietary ByteBuffer formats (MediaTek has tiled YUV
layouts at the kernel level, and NDK AMediaCodec lacks the Java API's
getOutputImage normalization - raw bytes plus format keys only). Probed
on the TV with user go-ahead: a standalone Rust probe (ndk crate 0.9,
media + api-level-26 features, armv7, run from /data/local/tmp) fed a
200-frame 1080p high-profile Annex-B H.264 stream (x264, AUDs for AU
splitting, in-band + csd SPS/PPS) to the platform video/avc decoder.

Buffer tap (the chosen path) - WORKS, format is honest:
- color-format=21 (COLOR_FormatYUV420SemiPlanar, NV12), stride=1920,
  slice-height=1088, buffer = 1920x1088x1.5 bytes exactly; crop keys and
  color-standard/range absent (assume BT.709/limited for HD content).
- Frame 60 pulled and reconstructed as plain NV12: pixel-perfect. No
  tiling, the reported format is the real layout.
- Throughput: 200 frames in 2.69 s = 74 fps at 1080p, ~3x realtime for
  25 fps content, decode-and-copy-out included.

AImageReader tap (the would-be fallback) - NOT AVAILABLE on this device:
AMediaCodec_start fails (ErrorUnknown) when configured with a
YUV_420_888 AImageReader window as output surface, with or without
CPU_READ_OFTEN usage. Old vendor HAL limitation: this decoder outputs to
composable surfaces or ByteBuffers, not CPU-readable gralloc consumers.
Fine - the primary tap needs no fallback here, but production code must
treat surface-attached taps as per-device-untrustworthy (consistent with
the Kodi/ExoPlayer surface-mode findings above).

Implementation notes from the probe: NV12 means the alloy YUV path
should take semi-planar (Y + interleaved UV) as a first-class layout,
which is also the single-texture-pair fast case (one R8 + one RG8
texture). slice-height (1088) != height: the UV plane starts at
stride*slice_height, and the bottom 8 rows are cropped by display size.
Absent color-standard keys mean the player defaults the matrix by
resolution (BT.709 for HD) unless the container says otherwise.

- https://developer.android.com/reference/android/media/MediaCodec
- https://lkml.iu.edu/hypermail/linux/kernel/2412.1/09112.html

## Decoder decision (2026-08-12): openh264 is the PoC, hardware per platform ships

openh264 is pure software decode and exists to prove the pipeline with the
smallest possible producer; it does not ship as the release path. Per
platform, the shipped decoder is the hardware one:

- Android: AMediaCodec buffer mode (probed, staging 2).
- Desktop Linux: VA-API - a STATELESS API, so the app owns bitstream
  parsing and DPB management; use the `cros-codecs` crate (ChromeOS's
  pure-Rust stateless decoder layer over cros-libva, BSD) rather than
  writing that ourselves.
- Pi 4: V4L2 stateful m2m (feed AUs, kernel codec parses -
  MediaCodec-like). Pi 5 dropped H.264 hardware decode entirely (HEVC
  only), so it runs the software fallback for H.264 regardless.
- macOS: VideoToolbox. Windows: Media Foundation.

Probed 2026-08-12 (scratchpad, before adoption): openh264's DECODER has no
B-slice support - it hard-errors on every B-frame (42/50 samples of a
default x264 encode; 50/50 with `-bf 0`). PoC/dev content must be encoded
without B-frames; real-world MP4s virtually always have them, which the
hardware decoders handle - one more reason they are the shipped path.
Also probed: symphonia's isomp4 reader DOES deliver H.264 sample packets
with timing, but does NOT surface the avcC extra data (SPS/PPS live only
there in MP4), so the `mp4` crate does demux (avcC, sync flags, per-track
timescales, AAC config) and symphonia is used purely as the AAC decoder
(ASC synthesized from the mp4 config; 88 packets -> exactly 88x1024 PCM
frames in the probe).

This also settles the H.264 patent question for desktop distribution:
Cisco's royalty-free scheme covers only their prebuilt binary, not
from-source builds, and MIT grants no patent rights - a bundled software
codec would put downstream commercial apps in the uncovered position.
Hardware/platform decoders carry the vendor's licensing (the Flutter
precedent: bundle no codec). openh264 stays a dev/examples fallback;
AV1/VP9 recommended for app-bundled content on codec-less devices.

## Open questions
- If 1080p upload cost ever shows in traces: the per-frame copy is the
  load [[texture-upload-staging]] anticipated; that item is the fix, not
  a new design here.

## Frame scheduling (2026-08-13): timeline clock + standing demand, behind `video-timeline-pacing`

Session findings on the TV (rebased on the [[frame-pacing-fluency]] fix,
SwapPaced active), traced with examples/video/src/probe/pacing.tsx (rebuilt
this session: 50fps/25fps synthetic bar clips, remote dpad switching via
gamepads() - the TV remote is joystick-classified, onKeyDown never sees it -
plus "pump"/"cadence" debug commands):

- Baseline: a 640x360 50fps clip on the 50Hz panel missed ~11% of present
  slots (SF census: mean 22.5ms with exact-40ms holes; engine: fps 42,
  skippedPerSec 19, 61 signals/s). Two stacked causes.
- Cause 1, demand: video produced frames only on its own uploads, so the
  loop free-ran off the vsync grid (ticks faster than 50/s with idle-tick
  gaps). Proven by a standing onFrame ("pump"): presents snapped to
  20.02ms / 0.04% drops - but content still stuttered.
- Cause 2, clock: the silent-stream master clock was Instant::now() read
  inside the tick. The tick's JS work executes at bursty wall moments
  (7.4% of gaps >30ms, pairs ~0ms apart; runtime frame counter never
  skips) even when presents are metronomic, so pts<=clock selection
  inherited the jitter: 11% held + 9% double-stepped frames measured by
  per-tick currentTime() sampling while presents were clean.
- Fix shipped behind `video-timeline-pacing` (flux feature, lattice
  passthrough, default OFF): silent streams clock on the engine timeline
  (flux::Clock = paced frame clock; new timeline_now_ms in
  standards/time.rs), selection gets a half-period lookahead (play()
  anchors the pts grid in phase with the tick grid; without the offset
  every comparison sits on a boundary where sub-ms timeline noise flips
  it), and video::tick returns VideoTick{uploaded, playing} so lattice
  keeps a standing frame request while any player is mid-playback
  (playing && !finished; a finished clip stops demanding). PacedClock
  gained period_ms(). Audio-clocked streams untouched (sink position
  stays master; needs the same treatment later - see below).
- Measured with the flag on: presents 20.05ms / 0.13% drops (from 11%);
  content steps per vsync 98.2% exact-20ms in a restart-free window.
- RESIDUAL, why the flag defaults OFF: user still sees ~1 hitch/s on the
  TV. Correlates with the known idle-tick oddity ([[frame-pacing-fluency]]
  flagged "~1.1 idle Ticks/s during continuous animation, should never
  happen"): idleTicks climbs ~1.1/s during playback, the loop runs 51-52
  iterations/s on the 50Hz panel (reusedPerSec 52 vs fps 50, cadence
  meanMs 19.69), and each stray idle tick runs an extra loop iteration
  that advances the paced clock one extra period (its one-period-per-call
  model) and presents duplicate content, shoving the next real frame a
  slot late. Invisible to the SF census (latch grid stays 20ms) and to
  position sampling (records a 0-step); visible to the eye.
- Next steps, in order: (1) alloy idle-tick gate - no idle tick while a
  standing frame request is in flight (suspect: race between the JS
  executor's request_frame and the loop's idle decision; alloy app.rs /
  vsync.rs). (2) Re-measure with the flag on; if a residual flap remains
  at pts-grid boundary crossings (~0.6% hold/jump pairs from paced-vs-pts
  phase drift), the prepared refinement is a per-player clock that
  advances exactly one period per tick and re-syncs to the timeline only
  when drift exceeds 1.5 periods (one clean slip instead of a flapping
  stretch; also keeps realtime under raster-bound ticking). (3) Stage 3:
  audio-clocked streams - smooth the PCM sink position onto the same
  timeline (it quantizes in audio-callback chunks).
- Measurement traps: the pacing probe's clip-restart remount freezes
  position ~1.3s every 60s (probe artifact, not pipeline); sum-of-steps
  deficit arithmetic is meaningless across a restart; engine stats fps
  and the SF census are both structurally blind to duplicate-content
  presents - only per-tick position sampling or the eye catches them.

### Amendment 2026-08-14: measure first, and make the advance idempotent

Discussion of why this class of bug is expensive, re-reading the code. The
next-steps list above stands, with two changes ahead of it and one
reframing. The harness and deadline halves of the same discussion went to
[[frame-driver-pacing-contract]], since they are frame-driver properties
that outlive video.

1. **Present ledger in alloy, first.** The trap above ("both structurally
   blind to duplicate-content presents") is the reason this trace keeps
   costing device sessions: the only reliable detector is the eye, and
   `reusedPerSec > fps` is an inference. alloy already knows the tuple at
   each present - frame index, the modeled paced timestamp, the raw wall
   reading at present return, and which content generation went out. A
   small lock-free ring of the last N presents, drained on read, makes a
   duplicate-content present a directly readable fact and gives the actual
   1-vs-2 vsync cadence instead of a reconstruction. Lock-free is the
   point, not an optimization: MCP debug calls run on the JS thread and
   cause the very drops being counted ([[diagnostics-off-raster-queue]] is
   the same asymmetry). This is also the assertion surface the harness in
   [[frame-driver-pacing-contract]] needs, so it is built once and used
   from both sides.

2. **Make the timeline advance idempotent, rather than gating the tick.**
   The mechanism, now traced: lattice/src/runtime.rs ticks `PacedClock` for
   idle Ticks as well as for presents, deliberately ("Idle Ticks arrive at
   the refresh cadence, so ticking the paced clock for them preserves its
   one-period-per-call model"). That is correct only while Tick and
   FrameRendered are mutually exclusive at the refresh cadence, which is
   what the gate in alloy/src/app.rs is supposed to guarantee (no Tick
   while `pending_presents > 0`, none while the raster queue is non-empty).
   It does not hold: ~1.1 stray Ticks/s. So the clock's one-period-per-call
   model is load-bearing on an invariant enforced in another crate, and
   every future source of an extra call is another instance of this bug.

   Structural version: `PresentClock::on_present` (alloy/src/present.rs)
   advances by `round((raw_ms - clock) / period)` periods instead of
   exactly one, clamped at zero or more, keeping the GAIN correction and
   the STALL_MS snap as they are. A second call inside the same refresh
   period then advances ~0, a genuinely skipped present advances 2, and
   both are what the timeline wants. The mutual-exclusion invariant stops
   being load-bearing for correctness.

   Risk to settle before shipping it: quantization flap near a boundary
   when the raw reading is noisy (0 / 1 / 0 / 2 instead of a steady 1).
   GAIN smoothing should hold it, but that is precisely a synthetic-vsync
   harness question, not a TV question.

   The idle-tick gate stays worth doing - an idle Tick during continuous
   animation is still wrong, and it costs a wasted loop iteration plus a
   duplicate present - but it becomes an efficiency fix rather than the
   thing correctness rests on. Note also why the symptom is duplicate
   content and not a harmless no-op: with the standing frame request a
   mid-playback player holds, an "idle" tick is never idle. It consumes
   the standing request and paints.

3. Then re-measure with the flag on, and only then the per-player
   quantized clock (item 2 of the list above) and audio-clock smoothing
   (item 3). Both keep their places; the point of the reordering is that
   neither can be evaluated honestly until the ledger exists.

## Staging

1. Bare minimum, desktop: forge::video (mp4 demux + openh264 + player),
   alloy YUV plane textures + conversion pass, flux gui/video.rs binding,
   @solidrt/core/video; play/pause only; verified in an example app on
   desktop. IMPLEMENTED 2026-08-12: alloy yuv.rs (NV12/I420 registry
   composition over plane textures + shader target, example
   alloy/examples/yuv_texture.rs), forge/src/video/ (demux/h264/aac/player,
   tests in forge/src/tests/video.rs with a B-frame-free fixture clip),
   alloy PCM sink (plain SDL3 stream in audio.rs, sink position = master
   clock), flux alloy_plugins/video.rs (flux:video, tick in lattice
   runtime.rs), packages/core/src/video.ts (createVideo), flux-types
   gui/video.d.ts, examples/video/.
2. Android producer: implement the AMediaCodec buffer-mode decoder (tap
   probed and verified 2026-08-12, NV12); fluency check at 1080p on the
   TV inside the real runtime. CODE DONE 2026-08-12:
   forge/src/video/mediacodec.rs (buffer-mode AMediaCodec behind the
   VideoDecoder trait; SPS/PPS as csd-0/csd-1 plus the demuxer's in-band
   copies - the probed-working combination; stride/slice-height/crop
   repack into the tightly packed contract during copy-out, color-format
   21 consumed natively and 19 interleaved so both emit NV12; EOS-drain
   flush; input dequeue drains ready outputs between attempts so full
   input queues cannot deadlock). Decoder selection is per platform in
   player.rs (no software fallback on Android - openh264 is excluded
   from Android builds entirely, so the C++ build is skipped there);
   forge::video::decoded_layout() replaces the demuxer's layout method
   (the layout is the decoder's fact, not the container's). ndk 0.9
   media + api-level-26 (no 28+ gated calls; crop read as i32 keys).
   armv7-linux-androideabi cross-check clean, desktop suite green.
   TV-VERIFIED 2026-08-12 (release runtime, dev-server flow, MCP stats):
   correctness PASSES - the 1080p25 High-profile B-frame clip
   (examples/video/assets/clip1080.mp4) decodes via MediaCodec buffer
   mode with correct BT.709 colors, upright orientation, and the
   burned-in timecode matching wall clock (A/V sync held; zero
   forge::video warnings, zero crashes over full-clip runs). Decode
   keeps pace: ~26 YUV conversion passes/s = every 25fps frame decoded,
   repacked, and uploaded on time. Fluency at 1080p FALLS SHORT on the
   presentation side: 16-19 fps shown (frameMs 43-59), raster thread
   ~820ms busy per wall second (~33ms/frame; the conversion pass is
   only 1.3ms of that, so the cost is the 3MB/frame upload into a
   texture the GPU sampled last frame plus the 1080p composite on the
   Mali-T860). At 640x360 the same pipeline runs the full 25 fps
   (frameMs ~26). Resolution-scaled raster cost, exactly the load
   [[texture-upload-staging]] anticipated: staging item 5 is now
   MEASURED-NEEDED for 1080p-on-TV, not speculative.
3. POSTPONED (user decision 2026-08-12): hardware decoders per remaining
   platform, replacing openh264 as the shipped path (decoder decision
   above): VA-API via cros-codecs on desktop Linux, V4L2 stateful on
   Pi 4, VideoToolbox on macOS, Media Foundation on Windows. One rung per
   platform, each behind the same decoder trait. Until then openh264 is
   the decoder everywhere except Android; the patent stance must be
   resolved before any desktop RELEASE ships video.
4. Seek, loop, playback rate; plane-texture exposure (tier 2) when
   something consumes it.
5. Only if measured insufficient: staging-buffer upload via
   [[texture-upload-staging]], and/or the opt-in surface-import rung.
   PICKED UP 2026-08-13 (the 1080p TV raster bound made it
   measured-needed): that item's stages 1+2 are implemented - update_yuv
   moves the owned frame across the raster channel (no per-plane copies)
   and uploads into double-buffered plane sets so no in-flight conversion
   pass is written under. TV re-measured 2026-08-13: raster busy -25%,
   correctness holds, but fps UNCHANGED at 17-19/25 - the limiter is
   critical-path latency against the 50 Hz vsync grid (details and the
   stage-3 rationale in that item), not raster capacity.
6. Frame scheduling: timeline clock + standing demand IMPLEMENTED
   2026-08-13 behind `video-timeline-pacing`, default OFF until the
   idle-tick residual is resolved (see the Frame scheduling section
   above for measurements, the residual chain, and next steps).
7. Audio unification (after the video PCM path is proven on desktop and
   the TV): replace SDL3_mixer with symphonia decode (Vorbis + WAV, and
   MP3/FLAC/AAC clips become feature flags) feeding one SDL3 audio
   stream per voice, SDL3 device mixing, per-stream gain via
   SDL_SetAudioStreamGain and panning as a multiply on push. Deletes the
   vendored sdl3-mixer-sys C dependency and unifies clip and video audio
   on one PCM path. Own verification pass: all clip-audio behavior on
   desktop AND Android targets.
