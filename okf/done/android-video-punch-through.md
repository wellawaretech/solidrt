---
title: Fullscreen video by surface punch-through on Android
description: Fullscreen VP9 playback decodes straight into its own SurfaceView, composited by SurfaceFlinger under a translucent UI, off our frame loop entirely. Round one (2026-09-12) is silent playback with play/pause/seek on the TV and the tablet; round two (2026-09-12, verified on the TV 2026-09-13) adds Opus audio from WebM, with the sink position correcting the clock anchor instead of selecting frames. Decided 2026-09-12, reversing the 2026-08-12 rejection; the texture pipeline keeps every non-fullscreen use.
created: 2026-09-12
completed: 2026-09-13
---

# Fullscreen video by surface punch-through on Android

Decided 2026-09-12, reversing the rejection recorded in
[[video-playback]]. This is a second path beside that pipeline, not a
replacement for it: video in a UI, video on a mesh, and anything that wants
a texture id keep going through the existing one. That pipeline is not
touched in this round; what it gets from here is a shared demuxer seek.

Round one, planned 2026-09-12: VP9 in MP4 files, no audio, fullscreen,
play / pause / seek / position / finished, verified on the Philips TV
first and the Samsung tablet second. Audio comes after the picture is
right, as its own round (the design below already says where the audio
clock plugs in). Live streaming is wanted and is the round after that, so
round one builds the seams it needs rather than the file-only shortcuts:
a demuxer trait over a byte source, a keyframe-based seek contract, a
transport module the plane player is thin over, and a clock that anchors
on the first frame and re-anchors after a stall. Adaptive streaming
(quality switching) is a possible later addition and changes nothing in
the app-facing surface.

## The rule underneath it: a surface is a clock

Sharing a surface means sharing a clock. Video pixels that have to appear in
the UI's composited frame are presented when that frame is presented, so the
video inherits the UI's cadence and its stalls. That is not an engineering
limit to chip away at, it is what compositing means, and it is why no amount
of shortening the chain below makes fullscreen 1080p fit.

What costs the clock is the shared surface, not the postprocessing. Access to
the pixels and ownership of the cadence are separate axes, which gives three
contracts rather than two:

- **Plane.** Raw decoder output on its own surface. Video clock, no pixel
  access, UI composited over it by SurfaceFlinger. This item.
- **Own surface, our rendering.** Decode to a texture, run shaders, render the
  result to a second surface on a video-rate loop. Video clock AND pixel
  access, but the video is still a rectangle the UI can only sit on top of: it
  cannot be a material in a scene or blend with UI content. Costs the free
  overlay, because the compositor now blends two rendered layers. Not built,
  named here so the axes stay apart and this does not get rediscovered later as
  "punch-through with shaders".
- **Texture.** Full composability, UI clock, no exceptions. The existing
  pipeline.

Two consequences worth stating plainly:

- Texture-path fluency is capped by UI fluency by construction. On a device
  whose loop iteration costs ~26 ms against a 20 ms period, fullscreen 1080p is
  outside that cap and always was. It is not a defect in the video pipeline,
  and no rung of the texture path can lift it.
- Everything scheduled against our frame loop is therefore correctly scoped as
  the TEXTURE path's clock - the timeline clock, the half-period lookahead,
  standing demand, [[frame-driver-pacing-contract]]. They make that contract as
  good as it can be and can never remove the coupling.

## Precedent: this is what browsers do

A browser never runs video on rAF. Demux and decode sit on their own threads,
`currentTime` comes from the audio renderer's clock, and the frame is handed to
the compositor thread as its own layer, so a stalled main thread freezes the
page and not the video. On top of that Chromium promotes video to a platform
overlay wherever one exists: DirectComposition on Windows, CALayer on macOS,
SurfaceView on Android. Fullscreen YouTube in Chrome on Android IS
punch-through, with the page punching the hole.

That also sets the prior for the device risk below. Android TV certification
requires VP9 decode, YouTube ships VP9 to TVs, and both YouTube and Netflix
present through SurfaceView in surface mode. On this silicon surface mode is
the vendor-exercised path and buffer mode is the unusual one. The Kodi colour
reports stay on the record, they are real and they are about this chip - but
Kodi wanted buffer mode for its own GL scalers, so they are not evidence that
surface mode is the fragile choice here.

## Why, in numbers

Fullscreen 1080p25 never became fluent on the Philips TPM171E through the
texture pipeline, and the 2026-09-12 measurements say why it cannot be
made to fit by shortening steps. Per loop iteration at 1080p, against a
20.0023 ms refresh period:

| | ms |
|---|---|
| plane upload (after staging; 27.5 before) | 10.4 |
| window draw | 11 - 13.5 |
| present | 4.5 |

The whole chain is on the critical path of one refresh period, and the
window draw does not shrink with the video's size: it is a full-window
repaint because a texture changing behind an unchanged id produces no
damage ([[live-texture-content-damage]]). 720p fits (49.2 presents a
second, 124 of 126 intervals on the grid, every frame delivered); 1080p
lands at 41.7 a second with a quarter of its intervals double-length.

Punch-through does not shorten that chain, it removes it. The decoder
renders into its own surface, SurfaceFlinger composites it, and none of
upload, conversion, repaint or our present cadence is involved at all.

## Container: WebM for files and live

The container never reaches the decoder: MediaCodec takes the coded VP9
frames the demuxer hands it, superframes included, with no parameter sets
and no rewriting. So the container is purely a demuxer question. Decided
2026-09-12: WebM for files and live alike, with Opus audio, replacing MP4.
The reasoning:

- **Files: WebM too, not MP4.** Round one read MP4
  (`forge/src/video/demux.rs`, `vp09`/`vpcC`). Keeping MP4 beside WebM means
  two demuxers, and Opus in MP4 is a sample entry the `mp4` crate (0.14.0)
  does not read; one container for files and live is one demuxer. Existing
  clips move by remux: the VP9 stream copies as is, AAC tracks re-encode to
  Opus.
- **Live: not plain MP4, ever.** A plain MP4's sample table is written when
  the file ends, so a reader needs the whole file before its first frame.
  Live needs a chunked container with no up-front table: WebM (Matroska,
  VP9's native container, an unknown-size segment of clusters, what ffmpeg
  emits with `-f webm` to a pipe or socket and what YouTube's live ingest
  took for years) or fragmented MP4 (CMAF, `moov` with empty tables plus
  `moof`/`mdat` fragments, what HLS and DASH segments are). WebM is the
  choice: it is designed for exactly this, it is small enough that a
  reader for our subset (EBML, Segment, Tracks, Cluster, SimpleBlock) is a
  few hundred lines if symphonia's Matroska reader turns out not to hand
  out video packets, and a WRITER is the same size again, which matters
  because the likely live producer is our own (the vendored libvpx already
  builds the VP9 encoder; camera, p2p and serve exist). The `mp4` crate's
  fragmented-MP4 reading is partial and it cannot write fragments at all.
  A probe of symphonia's Matroska reader decides, when the demuxer is built,
  whether we use it or our own subset reader; adaptive streaming later
  would pull toward CMAF only if the streams come from third-party
  HLS/DASH servers, which is not the case.
- **Audio is Opus (decided 2026-09-12).** WebM carries Opus or Vorbis,
  never AAC, and AAC is patent-pooled where VP9 and Opus are royalty-free.
  Opus over Vorbis: newer, better, and built for low latency, which live
  wants. Decode through libopus, vendored and hand-bound like libvpx; Android
  also has a platform Opus decoder (MediaCodec, since 5.0), and which one
  Android uses is the audio round's call.

What round one does about this: the demuxer becomes a trait, `Demuxer`
(`info`, `next_video`, `next_audio`, `seek`), over a byte source that may be
unbounded and non-seekable (a file, a fetch body, a p2p or socket stream).
The MP4 demuxer was its first implementation; the WebM demuxer replaces
it, arriving with audio rather than with live, because audio clips are WebM.
The player never sees a sample table. Android's own
`AMediaExtractor` is not used: it would give Android a container contract of
its own, and the point of one demuxer per container is that a stream plays
the same on every platform.

## Mechanism

`AMediaCodec` is configured with a SurfaceView's `ANativeWindow` as its
output surface. Decoded buffers are never mapped: `releaseOutputBufferAtTime`
hands the frame to the surface with a target presentation time, and
SurfaceFlinger latches it on the vsync that time falls in. The ndk 0.9 crate
already exposes everything needed at api-level 26: `configure` with a
`NativeWindow`, `release_output_buffer_at_time`, `flush`, and
`NativeWindow::from_surface` for the JNI `Surface`.

The clock the release time is expressed in is `CLOCK_MONOTONIC` (what
`System.nanoTime` reads). Content time maps to it by one anchor pair:
`t_sys = origin_sys + (pts - origin_pts)`, anchored on the first frame
released after play, resume or seek (never on pts 0: a stream starts
wherever it starts). A stall - the source or the decoder delivering nothing
for longer than `STALL_REANCHOR` - re-anchors on the next frame instead of
releasing the backlog as a burst of late frames; on a file that is the
difference between a hiccup and a skip, on a live stream it is rebuffering.
No audio in this round means nothing corrects the anchor; when audio
arrives, the sink position becomes the thing the anchor is corrected against
(a slope correction when drift exceeds a threshold), which is a different and
gentler use of the audio clock than the texture path's per-frame selection,
where the sink's callback-chunk quantisation drops ~10% of frames. Live adds
a latency target to the same anchor (play a fixed distance behind arrival,
re-anchor when the buffer drains or grows past it); the anchor module is
where that lands.

The anchor, play/pause/seek state, the command channel and the published
position/playing/finished live in their own engine-free module,
`forge::video::transport`, shared by both players. The plane player is that
module plus the codec release loop; the texture player adopts it when it
moves off the frame loop (below). Building the anchor inside the plane
player would have the texture path duplicate it later, which is the one
way round one could force a redesign.

Layering consequences, all decided:

- This is a SECOND decoder mode, not a change to
  `forge/src/video/mediacodec.rs`. That file stays as it is for the texture
  path.
- `VideoDecoder`'s contract (feed an access unit, collect planar frames) does
  not describe it: there are no frames to collect. The plane player is its own
  type, `forge::video::PlanePlayer`, Android only.
- Everything in the texture path's frame scheduling - the engine-timeline
  clock, the half-period lookahead, standing frame demand, the tail-frame
  release - is inert here. The codec and SurfaceFlinger own the timing, and a
  plane player is not visited by the per-frame `tick` at all.
- forge stays engine-free, but not window-free. The surface handle crosses
  into forge as an ndk `NativeWindow`, a platform type forge already speaks
  (it holds the `MediaCodec` from the same crate). Who creates it is settled
  below.

## Ownership: alloy owns the plane, forge owns the decoder, flux joins them

alloy owns the Android window, the SDL view and the JNI seam
(`alloy/src/sdl_utils.rs` already reaches the activity through
`SDL_GetAndroidJNIEnv` and the `jni` crate), so the second SurfaceView is
alloy's: creation, layout rect, z-order, teardown. forge owns decoding and
takes the `NativeWindow` as an argument. flux's video plugin is where the two
meet, as it already is for textures: `open` asks alloy for a plane, opens the
forge player on its window, and `close` tears both down in the right order
(player first, so the codec releases the surface before the view goes).

No new crate edge. alloy still does not depend on forge.

## Android glue: the one part that is not free

The decode path is independent of the texture path. The Android glue is
not, and it is the only cost punch-through adds rather than removes:

- **A second SurfaceView beneath SDL's.** Added to SDL's `mLayout` at index 0
  by a small helper in `SolidRTActivity` (create with a rect, set rect,
  destroy), called from native on the UI thread; the `Surface` comes back once
  `surfaceCreated` fires. SurfaceViews sit below the activity window by
  default; SDL's gets `setZOrderMediaOverlay(true)` so it sits above the video
  one. Both settings are made before the window attaches.
- **SDL's surface made translucent.** SDL's `SDLSurface` never calls
  `setFormat`, so today the layer is flagged opaque to SurfaceFlinger whatever
  its buffers hold. Three things have to line up for a hole: the holder format
  (`PixelFormat.TRANSLUCENT`), an EGL config with 8 alpha bits
  (`alloy/src/gl/context.rs` asked for RGB 8 and no alpha), and the window
  framebuffer carrying alpha 0 wherever the video should show. The third was
  already true: the Android fast path clears FBO 0 to transparent black every
  frame (`gl/draw.rs`), and the opaque black one sees today is the layer flag,
  not the pixels. So the first two are set once at startup and nothing flips
  at runtime; uncovered pixels show the plane while one exists and the
  activity's black window background otherwise, which is what they showed
  before.
- **A translucent main surface changes compositing for the WHOLE app**, not
  just screens that play video. This is the one regression the design can
  cause and it would hit every app, so it is measured on the TV as the first
  checkpoint (below), before the decoder lands. If HWC drops the layer to
  client (GPU) composition, the fallbacks are, in order: flip the format only
  around a plane's lifetime (pays an SDL surface re-create on entry and exit),
  or put the video ABOVE the UI and hide the UI while it plays.
- **JNI, and meddling with SDL's Android view** - one of the original
  objections, now accepted.

## Device history, for the record

Surface mode is what YouTube, Netflix and Kodi use on this TV, and all
three play fine there, so the path is known to work on this chip. The
notes that once read as risk stay on the record because they explain
choices made in August, not because they gate anything now:

- The documented Philips colour corruption (pink/purple) was in Kodi's
  SURFACE mode; Kodi's buffer mode was wanted for its own GL scalers.
- ExoPlayer carries MediaTek-specific surface workarounds
  (`codecNeedsSetOutputSurfaceWorkaround`: re-create the codec instead of
  `setOutputSurface`). We configure the surface once and never swap it.
- Our 2026-08-12 probe found `AMediaCodec_start` failing with an AImageReader
  window as output: a CPU-readable consumer, which a SurfaceView is not.

There is no standalone probe this time. Every piece a probe would need is a
piece of the real path, so the first on-device run of the real path confirms
it, with nothing thrown away.

## Shape (decided 2026-09-12)

The codec's output mode is fixed at `configure`, surface or buffer, so the
contract is a property of the player chosen at open:

```ts
let plane = await open(source, { present: "plane", fit: "contain" })  // Android only
let player = await open(source)                                       // texture, unchanged
```

`source` is a path in this round; a URL or a stream handle is the live
round's addition to the same argument, and nothing else in the app-facing
surface changes for live or, later, adaptive: no video primitive, the same
player object, `duration` undefined and `seek` a no-op on a stream without
an end, `finished` when the stream ends. Quality switching is internal to
the player.

A plane player has no `texture` - a hole, not a picture. The binding IS the
player's lifetime: the plane exists fullscreen from `open` until `close`,
which is also what removes the trap of tearing it down on pause. There is
no separate window prop and nothing to bind; a player screen opens the
plane in its component and `onCleanup` closes it. One plane at a time: a
second plane open while one exists rejects. On any platform without a plane
`open` with `present: "plane"` rejects too; an app that wants to run
everywhere falls back to the texture player itself. The transparent
fallback ("the same option presents the texture fullscreen behind the UI")
is a later addition, not part of this round.

`fit` is presentation and costs nothing: the SurfaceView's layout rect is the
letterboxed (`contain`) or cropping (`cover`) rect of the video inside the
window, computed by alloy from the video size and the window size in pixels
and re-laid-out on a window resize. The compositor scales the decoder output
to the rect; the bars are the activity's black window background showing
through the UI's transparent clear.

The `@solidrt/core/video` wrapper follows the same option: `createVideo(path,
{ present: "plane" })` returns the same `VideoStream` with `texture()`
forever undefined, plus `ready()` (open resolved) and `seek`.

## Transport, this round

`play`, `pause`, `seek(t)`, `currentTime()`, `finished()`, `close`. `duration`,
`width`, `height` as today. On a plane each means:

- **Play**: anchor `origin_sys = now`, `origin_pts = position`, start releasing.
- **Pause**: stop releasing. Decoded buffers stay in the codec, the surface
  keeps its last latched frame, and the UI layer goes idle over a frozen
  picture: we present nothing at all.
- **Seek**: flush the codec, ask the demuxer to reposition (its contract:
  the last keyframe at or before `t`, or the next keyframe when the source
  cannot go back), decode forward releasing nothing (`render = false`) until
  the first frame at or after `t`, release that one immediately (so a paused
  seek shows the target frame), and re-anchor. The latched frame survives
  the flush, so the old picture holds until the new one lands. How the
  keyframe is found is the demuxer's business (a table lookup in MP4, a
  cluster scan in WebM); the player is the same for both.
- **Position**: the pts of the last released frame, published by the worker.
- **Finished**: the end-of-stream buffer has come out and every earlier frame
  was released.

Rate (including keyframe-only trick play above 2x) and single-frame `step`
are not in this round. Both are cheap on a plane - the anchor gets a slope,
step is one release while paused - and are listed under follow-ups so they
are designed once, for both contracts.

The release schedule, in the worker (ExoPlayer's policy, one threshold each):

- A frame due more than `RELEASE_LEAD` (two refresh periods) ahead waits;
  the worker sleeps until it is inside the lead, then releases it at its
  time. Releasing further ahead than the BufferQueue holds would block the
  codec on its own output.
- A frame inside the lead, or late by less than `DROP_LATE`, is released at
  its time (SurfaceFlinger shows a slightly late frame on the next vsync).
- A frame later than `DROP_LATE` is released with `render = false`. This is
  the only frame drop the plane path has, and it only happens when decode
  cannot keep up, which on the TV (vendor-measured 110-200 fps at 1080p) it
  can.
- Every release time is snapped onto the display's vsync grid first (added
  2026-09-12 after the tablet census below): the closest vsync to the
  anchored due time, minus `VSYNC_OFFSET_PERCENT` (80%) of a period,
  ExoPlayer's `VideoFrameReleaseHelper` policy. The compositor latches a
  buffer whose desired present time is at or before the vsync it composes
  for, so a time just past a vsync slips a whole period; pulling it most of
  a period back keeps it on the intended vsync whatever the phase offsets.
  The grid is `transport::VsyncGrid`: the period from
  `Display.getRefreshRate()` (read once at plane creation), the phase from
  a `Choreographer` frame callback the plane view keeps re-posting while it
  is attached (one JNI upcall per display frame into an alloy static, the
  same shape as the surface-lost report). Fresh samples keep the
  extrapolation to a couple of periods, so a rounded period cannot drift.
  forge stays JNI-free: it takes the period and a sampler closure.

## The worker, and what crosses threads

`PlanePlayer::open(path, window)` spawns one thread that owns the demuxer,
the codec and the anchor. It never blocks the JS thread: codec construction
runs on the worker, as the texture path already does, and with the same
retry across a clip handover (the TV allows two VP9 instances; Solid's keyed
swap builds the incoming branch before disposing the outgoing one).

Commands cross from JS as an enum on a channel (Play, Pause, Seek, Close);
state crosses back as atomics (position, playing, finished). No per-frame
work on the JS thread, no event bus, nothing in the render loop. The JS
plugin polls the atomics on `currentTime()`/`finished()` exactly as the
example app does today with a timer.

The hole is open from `open` (transparent clear) rather than from the first
latched frame. The black flash that gating on the first frame would remove
is a black frame over a black player screen, so this round does not build
the gate; it is listed as a follow-up with the mechanism (a "first frame
released" flag the plugin reads on close-over).

## The texture path, browser style, later: additive on this

Video in a texture also belongs off the frame loop, the way a browser does
it: demux, decode, clock and frame selection on their own threads, and the
compositor sampling the latest due frame at its own cadence. The picture is
still presented at the UI's cadence - a shared surface is a shared clock,
and that is true in the browser too - but nothing about the video runs in
the JS tick or on the JS thread.

Today the texture path does its selection and its upload handoff inside the
JS tick (`flux/src/alloy_plugins/video.rs::tick`, driven from
`frame::advance`). Browser-style is: the worker pushes (frame, due system
time) to the YUV texture through the raster channel, the raster thread
latches the newest due frame at paint time against the same monotonic
clock, alloy holds the frame demand itself while a texture player plays,
and the JS tick drops out. That is one new latch mode on the YUV texture in
alloy plus a deletion in flux; the transport module, the demuxer trait and
the `open` surface are the same ones the plane player uses. Nothing in this
round is redesigned by it, which is why the transport lives outside the
plane player.

## Seams and lifecycle

- `close` order: player first (joins the worker, which stops and releases
  the codec), then the SurfaceView, then the clear goes opaque again on the
  next present. That present is requested explicitly; nothing else changes
  in the tree.
- Backgrounding destroys Android surfaces. In this round a destroyed plane
  surface ends playback (`finished` goes true, the player is closed under
  the app); the app reopens on resume via [[app-lifecycle-events]]. Keeping
  the position across a background trip is a follow-up (okf/tiny.md,
  Runtime).
- Window resize (tablet rotation) re-lays-out the plane rect; the codec is
  untouched.
- While a plane player exists the UI holds NO standing frame demand from
  video: the UI presents only when the UI changes. On the TV that is the
  difference between 50 full-window repaints a second and none, which is
  the second half of the win and the half the numbers above do not show.

## Staging

Every rung is real code; the measurements are checkpoints inside it.

1. **Plane in the activity, translucent UI.** `VideoPlaneView` (a
   SurfaceView measuring itself to the video's aspect, `contain` or `cover`,
   centered by the layout) plus create / destroy on `SolidRTActivity` and
   the SDL surface made translucent and media-overlay at its creation; the
   surface-lost native in `lattice/src/lib.rs` forwarding to alloy as the
   keyboard ones do; `alloy::video_plane::VideoPlane` (Android only: JNI
   create blocking until the surface exists, `NativeWindow` out); EGL alpha
   8. CHECKPOINT on the TV before step 2:
   `dumpsys SurfaceFlinger` composition type per layer and present latency on
   an app that plays no video, against today's numbers. This gates the
   translucency design; the fallbacks are named above.
2. **`forge::video::transport` and `PlanePlayer`.** The `Demuxer` trait
   over a byte source with the MP4 implementation and its keyframe seek;
   the transport module (anchor, first-frame anchoring, stall re-anchor,
   play/pause/seek state, command channel, published state); the plane
   player as transport plus surface-mode codec setup and the release loop.
   The trait and the transport are what the live round and the texture
   path inherit.
3. **`flux:video` and core.** `open(path, { present, fit })` overload
   returning the plane player object; `createVideo` passthrough with
   `ready()` and `seek`; `flux-types` and the core types; the plugin owns
   the alloy-plane-then-forge-player open and the reverse close.
4. **Example and device verification.** `examples/video` gets a plane page
   (remote: select = play/pause, left/right = seek 10 s, back = leave) and
   is verified on the TV first, the tablet second - see "Done looks like".

## Verification, on device

Build: `make -C lattice android-client VIDEO=1 ANDROID_ABI=<abi>`, gradle
`assembleGoDebug -PsolAbi=<abi>`, `adb -s <serial> install -r`. The TV is
armeabi-v7a, the tablet arm64-v8a. Run through the dev server with `--lan`
and `sol android`; the user drives the device, and the numbers are read
off the device without touching it:

- `adb shell dumpsys SurfaceFlinger --latency <video layer>`: the VIDEO
  layer's present census, not ours. 1080p25 on the 49.9942 Hz panel should
  show intervals of two periods (40.005 ms) with no singles or triples.
- The SDL layer's census while the UI is static: presents ~0 a second.
- `logcat` for the codec name (`OMX.MTK.VIDEO.DECODER.VP9` on the TV,
  `OMX.qcom.video.decoder.vp9` on the tablet; the tablet's decoder has a
  128x128 minimum, smaller clips fall to software silently).
- Colours by eye (the MediaTek risk); a moving control drawn over the video
  by eye (the overlay works); pause holds the frame; seek lands within one
  keyframe interval of the target.

## Done looks like

Fullscreen 1080p25 on the TV, every frame on the 40 ms grid read off the
SurfaceFlinger census for the VIDEO layer; correct colours; UI composited
over it, with the UI layer presenting nothing while the UI is static;
play/pause/seek working from the remote; an app that plays no video
composited exactly as before (same HWC layer types, same present latency).
The same on the tablet, at 60 Hz, as a secondary check.

## Findings

Cut into [android-video-plane-compositing](../notes/android-video-plane-compositing.md).

## Round two: audio (built 2026-09-12)

The audio round, as the design above said it would land: audio never
selects frames, it corrects the anchor. What was built, and the decisions
taken on the way:

- **Container: our own WebM reader** (`forge/src/video/webm.rs`, ~400
  lines): the EBML subset ffmpeg and a future writer of ours produce (EBML
  header, Segment, SeekHead, Info, Tracks with the VP9 and Opus entries,
  Cues, Cluster with SimpleBlock and BlockGroup), unknown-size Segment and
  Cluster accepted for the live round. `symphonia-format-mkv` was measured
  first: it reads the files fine, but it cannot be used without
  symphonia-core, common and metadata, and links at 265 KB (fat LTO,
  stripped) for a reader we would use 6k lines of, more than half of
  libvpx's whole VP9 decoder. MP4, the `mp4` crate, symphonia and the AAC
  decoder are gone; the texture player reads the same WebM reader and Opus
  decoder through the `Demuxer` trait, its frame scheduling untouched.
- **Opus: the vendored libopus** (`forge/vendor/opus` at v1.5.2, built by
  build.rs through the `cmake` crate that already builds SDL, five
  functions bound by hand in `forge/src/video/opus/ffi.rs`), on Android
  too. There is no Opus hardware: Android's `audio/opus` MediaCodec is
  libopus behind the codec framework, so going through it would cost
  buffer copies and gain nothing. Decode cost is one to two percent of a
  core at 48 kHz stereo.
- **Trims by one rule.** A WebM block time counts from the first shown
  sample, so a packet's first sample sits pre-skip before its block time;
  the audio track (`forge/src/video/audio.rs`) tags each decoded chunk
  with that start time and discards samples before a discard point: 0 at
  the start of the stream (that is the pre-skip), the target after a seek
  (that is the preroll: the demuxer resumes audio from `target -
  SeekPreRoll`, 80 ms, so the decoder has converged by the target).
- **The sink across threads.** forge defines `AudioSink` (push, queued,
  position, pause, clear); alloy hands out a `PcmSinkHandle` over the
  SDL audio stream, which SDL documents as safe from any thread, and the
  flux plugin adapts one to the other. The registry keeps ownership and
  destroys the sink after the player's worker has been joined (the plane
  entry's teardown order). The plane worker tops the sink up to 500 ms
  between frame releases, whatever the play state, so play starts with
  audio ready and a seek's preroll decodes while the picture holds.
- **The clock correction** (`transport::AudioSync`): per released frame,
  the audio clock's lead over the anchor is smoothed over ~8 frames (the
  sink position advances a device buffer at a time, a sawtooth that must
  never look like drift) and, above 40 ms, the anchor moves by it once.
  Not a slope: with release times snapped to the vsync grid a slow slope
  lands as the same one-period step, only later. Crystal drift between the
  DAC and CLOCK_MONOTONIC is tens of ppm, so this fires every several
  minutes at most, and each firing is logged. `AUDIO_OUTPUT_LATENCY_US`
  is the net output offset (sound path minus picture path, which is about
  two vsyncs from release to scan-out); the picture is held back by it
  when the sound starts. Not measurable through SDL, so it is set with
  `examples/video/assets/avsync.webm` (flash + beep every 2 s): 60 ms on
  the Philips TV's own speakers (2026-09-13). An external speaker path
  differs and needs an app-level audio delay setting (parked in tiny.md).
- **Sound and picture start together.** The sink starts on the first frame
  released after play or a seek, not on the play command: MediaCodec takes
  up to a few hundred ms to produce that frame on the TV, and a sound
  started before it led by that much. A lead above the stall threshold
  then made every corrected frame re-anchor behind the sound for good,
  which is also why one anchor move is capped at half that threshold.
- **Not in this round:** volume and mute (the sink has gain; no JS surface
  yet; okf/tiny.md, Flux), rate (audio above 1x drops out unless
  time-stretched, not planned; [[video-playback]]), the texture path's own
  audio-clock smoothing (its audio moved onto the shared worker with
  [[video-texture-off-frame-loop]]).

Verification: on the TV (2026-09-13) lip sync is on the flash with the
sync clip and the crawl clip, with zero anchor moves logged over a run.
The checks left open here (the vsync-interval pattern with audio on, the
tablet, and pause, resume and seek by hand) were run on both devices on
2026-09-13 as part of [[video-streaming]]'s verification (its Status
section has the runs): the VIDEO census held two periods on the TV with
avsync.webm (one run had a single 3-then-1 pair) and the 2/3 alternation on
the tablet with the served clip, and pause, resume and seek were driven on
both with the sound in step (two TV starts logged a correction just past
the threshold, okf/tiny.md).

## Follow-ups

Each lives on elsewhere:

- Streaming from a URL, mid-stream resolution changes and adaptive streaming:
  [[video-streaming]] (live streaming is its deferred mode 2, which also
  names the WebM writer our own producer would need).
- Rate, step and settable `currentTime` on both players, and the plane's
  presentation off Android: [[video-playback]].
- The texture path off the frame loop: built as
  [[video-texture-off-frame-loop]].
- The position across a background trip: okf/tiny.md, Runtime. Volume and
  mute: okf/tiny.md, Flux.
- The hole opening on the first latched frame (and closing before
  teardown), and a display mode matching the clip on displays with several
  modes: okf/ideas.md.
- An app animating over the plane held to 16 fps: [[plane-adaptive-fence-wait]].

Related: [[video-playback]], [[video-streaming]],
[[live-texture-content-damage]], [[texture-upload-leases]],
[[frame-driver-pacing-contract]], [[app-lifecycle-events]].
