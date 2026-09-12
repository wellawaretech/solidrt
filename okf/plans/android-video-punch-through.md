---
title: Fullscreen video by surface punch-through on Android
description: Fullscreen VP9 playback decodes straight into its own SurfaceView, composited by SurfaceFlinger under a translucent UI, off our frame loop entirely. Round one (planned 2026-09-12) is silent playback with play/pause/seek on the TV and the tablet; audio and the texture path's transport come after. Decided 2026-09-12, reversing the 2026-08-12 rejection; the texture pipeline keeps every non-fullscreen use and is not touched in this round.
created: 2026-09-12
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

## Container: MP4 for files, a chunked container for live

The container never reaches the decoder: MediaCodec takes the coded VP9
frames the demuxer hands it, superframes included, with no parameter sets
and no rewriting. So the container is purely a demuxer question, and the
answer differs by source:

- **Files: MP4, as today.** `forge/src/video/demux.rs` reads `vp09`/`vpcC`
  (profile, bit depth, chroma, range), the sample table is in memory after
  `open`, and every clip and fixture is already in it. Nothing to redo.
- **Live: not plain MP4, ever.** A plain MP4's sample table is written when
  the file ends, so a reader needs the whole file before its first frame.
  Live needs a chunked container with no up-front table: WebM (Matroska,
  VP9's native container, an unknown-size segment of clusters, what ffmpeg
  emits with `-f webm` to a pipe or socket and what YouTube's live ingest
  took for years) or fragmented MP4 (CMAF, `moov` with empty tables plus
  `moof`/`mdat` fragments, what HLS and DASH segments are). WebM is the
  candidate: it is designed for exactly this, it is small enough that a
  reader for our subset (EBML, Segment, Tracks, Cluster, SimpleBlock) is a
  few hundred lines if symphonia's Matroska reader turns out not to hand
  out video packets, and a WRITER is the same size again, which matters
  because the likely live producer is our own (the vendored libvpx already
  builds the VP9 encoder; camera, p2p and serve exist). The `mp4` crate's
  fragmented-MP4 reading is partial and it cannot write fragments at all.
  The decision is taken when the live round starts, on a probe of the
  reader; adaptive streaming later would pull toward CMAF only if the
  streams come from third-party HLS/DASH servers, which is not the case.
- **Audio in WebM is Opus or Vorbis, not AAC.** symphonia decodes Vorbis;
  Opus would be libopus, vendored and hand-bound like libvpx. That is the
  audio round's question, noted here so the container choice does not
  forget it.

What round one does about this: the demuxer becomes a trait, `Demuxer`
(`info`, `next_video`, `next_audio`, `seek`), over a byte source that may be
unbounded and non-seekable (a file, a fetch body, a p2p or socket stream).
The MP4 demuxer is its first implementation; WebM is the second, added in
the live round. The player never sees a sample table. Android's own
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
  the position across a background trip is a follow-up.
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
`assembleGoDebug -PsrtAbi=<abi>`, `adb -s <serial> install -r`. The TV is
armeabi-v7a, the tablet arm64-v8a. Run through the dev server with `--lan`
and `srt android`; the user drives the device, and the numbers are read
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

Appended as the work goes (okf/README.md: cut into notes/ when the plan
closes).

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
- 2026-09-12, tablet, vsync snap built (see the release schedule above).
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

## Follow-ups (not this round, in likely order)

- **Audio.** AAC decode and the PCM sink as on the texture path; the sink
  position corrects the anchor (slope, thresholded), it does not select
  frames. Above 1x audio drops out unless time-stretched, which is not
  planned. If live lands on WebM, its audio is Opus (libopus, vendored and
  hand-bound) or Vorbis (symphonia).
- **Live streaming.** The second `Demuxer` (WebM, after a probe of
  symphonia's Matroska reader; own subset reader otherwise), a byte source
  over fetch and p2p, a latency target on the anchor, `open` taking a URL
  or stream handle. Resolution changes mid-stream are free on a plane. A
  WebM writer of the same subset is what makes our own producer possible.
- **Adaptive streaming**, maybe, after live: manifest, segment fetching
  and a bitrate policy inside the player; no change to the app surface.
- **Transport on both contracts.** `rate` (anchor slope; keyframe-only trick
  play above 2x), `step` while paused, settable `currentTime`; the texture
  path gets `seek` from the shared demuxer trait. Designed once, for both.
- **The texture path off the frame loop**, per the section above.
- **Hole opens on the first latched frame**, and closes before teardown.
- **Position across a background trip** (surface destroyed and re-created).
- **Fallback off Android**: `present: "plane"` presenting the texture player
  fullscreen behind the UI, so the same app runs everywhere.

Related: [[video-playback]], [[live-texture-content-damage]],
[[texture-upload-leases]], [[frame-driver-pacing-contract]],
[[app-lifecycle-events]].
