---
type: backlog-item
title: Video playback
description: One decode-to-YUV pipeline on every platform (software decoders on desktop, MediaCodec buffer mode on Android), planar YUV textures + shader conversion in alloy, player core in forge, no video primitive - texture/d-texture display the player's texture id. Fluency target is the Philips MT5891 TV; punch-through rejected. Probed 2026-08-12: the MTK decoder emits honest NV12 in buffer mode at 3x realtime for 1080p; the AImageReader fallback tap is unsupported on the device (not needed).
status: open
timestamp: 2026-08-12T00:00:00Z
---

# Video playback

Designed 2026-08-12 in discussion; decisions below are settled, the module
internals wait on one on-device probe.

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
- flux `plugins/gui/video.rs` - thin marshal + a per-tick hook (camera
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

## Staging

1. Bare minimum, desktop: forge::video (mp4 demux + openh264 + player),
   alloy YUV plane textures + conversion pass, flux gui/video.rs binding,
   @solidrt/core/video; play/pause only; verified in an example app on
   desktop. IMPLEMENTED 2026-08-12: alloy yuv.rs (NV12/I420 registry
   composition over plane textures + shader target, example
   alloy/examples/yuv_texture.rs), forge/src/video/ (demux/h264/aac/player,
   tests in forge/src/tests/video.rs with a B-frame-free fixture clip),
   alloy PCM sink (plain SDL3 stream in audio.rs, sink position = master
   clock), flux plugins/gui/video.rs (flux:video, tick in lattice
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
6. Audio unification (after the video PCM path is proven on desktop and
   the TV): replace SDL3_mixer with symphonia decode (Vorbis + WAV, and
   MP3/FLAC/AAC clips become feature flags) feeding one SDL3 audio
   stream per voice, SDL3 device mixing, per-stream gain via
   SDL_SetAudioStreamGain and panning as a multiply on push. Deletes the
   vendored sdl3-mixer-sys C dependency and unifies clip and video audio
   on one PCM path. Own verification pass: all clip-audio behavior on
   desktop AND Android targets.
