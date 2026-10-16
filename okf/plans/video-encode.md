---
title: Encode video and audio
description: The runtime decodes VP9 and Opus but cannot encode either, so an app cannot record its camera and microphone to a file or send them anywhere; a VP9 encoder (libvpx off Android, MediaCodec on it), an Opus encoder and a WebM writer in forge, fed natively from the camera tap and the microphone ring, surfaced as `record()` in flux:video; recording to a file first, the window as a second source, video chat over p2p as the later consumer of the same pipe.
created: 2026-10-16
---

# Encode video and audio

## Symptom

An app can show its camera (the frame tap, [[camera-frame-tap]]) and read
its microphone (the ring, [[microphone-tap]]), and it can play VP9 + Opus in
WebM ([[video-playback]], [[video-streaming]]). It cannot do the step in
between: turn frames and samples into compressed packets. So there is no
way to record a clip to a file, no screencast, and no way to send a camera
to another device. The only route today is reading pixels into JS and
shipping them raw, which no network and no frame budget carries.

## Decided (2026-10-16)

- **Record to a file first.** Video chat sets the hard requirements
  (realtime encode on a phone, keyframe on demand, bitrate that follows the
  link) and is the later consumer; the first app-visible feature is a clip
  recorded from camera and microphone to a WebM file, nothing crossing JS.
  The chat adds the live form of the same writer, a transport and the
  receiving player, not another encoder.
- **The surface is `record()` in `flux:video`,** in the idiom of the other
  media modules (an open-style call resolving to a bound handle with
  function accessors), not a `MediaRecorder` class with event properties.
  Encoding is video, so it lives beside playback.
- **The window is a source like the camera,** named by a node: `{ node }`
  in the same `video` slot a camera session fills (decided 2026-10-16, so
  the slot stays one picture source, not a flag per kind). The window node
  records the whole window as it is on screen; any other node records its
  painted box cut out of the window, as a cut-out (overlapping content and
  the window shader included), following the box when it moves. The
  readback is from FBO 0 on the raster thread into a latest-wins tap, the
  camera tap's type, asynchronous (pixel pack buffer, fenced, mapped a
  frame later), the downscale and the cut in the blit, never a JS
  `readTexture` per frame. The encoded size is `width`/`height` in the
  options, a recording's property, not a source's. A snapshot boundary's
  texture as a source (unshaded, the subtree alone) is deferred until a
  need arises; the `{ texture }` form already covers its shape.
- **`video-encode` is a feature at every layer,** implying `video`, in
  `default` and in `go` as [[module-feature-flags]] decided; the 1.28 MB
  encoder is in every stock runtime, measured as one size row.

## The surface

```ts
import { open, record } from "flux:video"
import { file } from "flux:fs"

let rec = await record({ video: cam, audio: mic }, {
  to: file("clip.webm"),
  videoBitrate: 1_000_000,   // bits per second, default from the frame size
  audioBitrate: 32_000,      // default for mono voice
  frameRate: 30,             // nominal; frames keep their real timestamps
})
let cast = await record({ video: { node: windowNode.id } }, {
  to: file("screen.webm"),
  height: 720,               // the width follows the window's aspect
})
rec.recording()              // false while paused and after stop
rec.duration()               // seconds of media written so far
rec.pause(); rec.resume()
await rec.stop()             // resolves once the file is final (sizes and Cues patched)
rec.error()                  // VideoError | null, like a player's
```

- Sources are the session objects the app holds, `CameraSession` and
  `MicrophoneSession`, either one optional; a node (`{ node: id }`, the id
  `captureSnapshot` takes) in the `video` slot is the window, or a cut-out
  of it.
- `record` resolves when the encoders are up and the file is open, and
  rejects with a `VideoError`: `unsupported` for a missing encoder or a
  microphone rate Opus does not take, `network` for a destination that
  cannot be written.
- Simplified contract: the microphone is at an Opus rate (8, 12, 16, 24 or
  48 kHz; the default 16 kHz is), so there is no resampler. The camera's
  open size is the clip size (a `width`/`height` that differs rejects); a
  node source scales into the size asked, fixed for the clip. A source
  faster than the encoder loses frames rather than queueing them, so a
  clip never runs slow. A window at rest adds no frames: the clip holds
  the last one, and a recording's start demands one frame so it begins
  with the screen as it is.
- The live form, `to` omitted and a `ReadableStream` of WebM bytes on the
  handle, pipes into a p2p stream's writable. Designed with the transport,
  not before.

## What is already here

- **libvpx** is vendored (`forge/vendor/libvpx`, v1.17.0) and built off
  Android with the VP9 decoder; the encoder is configured out in
  `forge/build.rs` because it costs 1.28 MB of linked binary (1732 KiB with
  it, 448 KiB without, [[runtime-optimization]]). The decoder ffi in
  `forge/src/video/vpx/ffi.rs` is hand-written and ABI-pinned; the encoder
  joins it (`vpx_codec_vp9_cx`, the `vpx_encoder.h` entry points).
- **libopus** is vendored and built whole through its CMake; only the
  decoder is bound (`forge/src/audio/opus/ffi.rs`). The encoder is in the
  library already, so Opus costs only its ffi and whatever the linker now
  keeps.
- **MediaCodec** is bound in `forge/src/video/mediacodec.rs` for decoding;
  an encoder is the same API in the other direction
  (`MediaCodec::from_encoder_type` exists in ndk 0.9).
- **The WebM reader** (`forge/src/video/webm.rs`) is ours and already
  accepts an unknown-size Segment of unknown-size Clusters (the live form);
  there is no writer.
- **The camera tap** (`alloy::camera::FrameTap`) publishes upright RGBA
  frames with SDL tick timestamps; **the microphone ring**
  (`alloy::microphone::MicrophoneTap`) is read by cursor from any thread
  and its `opened_ns` is on the same timeline, so sync costs nothing.
- **Live playback** (mode 2) is designed but deferred in
  [[video-streaming]]; the receiving end of a chat or screencast needs it.

## Shape

Layering by the usual question: a flux CLI with no window could encode a
file's frames, so the encoders and the writer are forge. The taps are
alloy. They meet on plain values (an RGBA frame with its timestamp, a block
of PCM with its sample position), carried by two feeder threads in the
plugin.

### forge

- `video::vpx::Vp9Encoder` (off Android): width, height, frame rate,
  bitrate, keyframe interval and a mode, `Recording` (one-pass VBR) or
  `Live` (CBR, error resilient). Realtime deadline, no lag, auto-alt-ref
  off (so a packet is one shown frame), row-mt, tile columns by width,
  threads capped like the decoder. Timebase in microseconds so timestamps
  pass through. `encode(i420, pts_us, force_keyframe) -> Vec<Packet>`,
  `flush`, `set_bitrate`. build.rs configures the encoder in under
  `video-encode`, in a libvpx build directory of its own.
- `video::encode`: `EncoderConfig`, `Packet`, the `VideoEncoder` contract
  every backend meets, `open_encoder` picking the platform's, and
  `rgba_to_i420` (BT.601 studio range, written into the track's Colour
  element so the player reads it back the same way).
- `audio::OpusEncoder`: at the microphone's rate, `OPUS_APPLICATION_AUDIO`
  for recording (`VOIP` for the chat), 20 ms frames; the lookahead becomes
  the OpusHead pre-skip.
- `video::webm::writer`: EBML header, Segment, SeekHead with reserved
  space, Info, Tracks (VP9 with Colour; Opus with OpusHead, CodecDelay,
  SeekPreRoll), a Cluster per video keyframe with a maximum length so block
  offsets stay in range, SimpleBlocks, Cues at the tail. Over `Write +
  Seek` the sizes, Duration and SeekHead are patched at close; over plain
  `Write` they stay unknown, what the reader's live path accepts.
- `video::record::Recorder`: one thread owning the encoders and the
  writer, fed over a bounded channel of plain values. It anchors time on
  the first input, subtracts accumulated pause time, cuts audio into 20 ms
  frames and muxes. A full channel drops a video frame and never an audio
  block. `stop` joins and finalizes.

### The pipe and the plugin

`flux/src/alloy_plugins/video.rs` exports `record`: two feeder threads per
recording, one on `FrameTap::wait_newer`, one on `MicrophoneTap::wait`,
forwarding to the recorder. Engine teardown stops every open recording
with a bounded join and the writer closes the file; nothing outlives the
engine. Types in `gui/video.d.ts`, `"video-encode"` in the gui
capabilities.

### The window tap (alloy)

After a frame's draw (the overlay included) the raster thread blits the
rect out of FBO 0 (a multisampled FBO 0, Android's in-tile path, resolves
1:1 into a retained intermediate first, since a scaled blit cannot
resolve) into a single-sample target at the tap's size, `LINEAR`, reads it
into a pixel pack buffer behind a fence, and maps the buffer at the next
frame, or after one refresh period when no frame follows (the raster loop
waits with a timeout while a readback is pending), reversing the rows in
the copy into the tap's buffer: no reversed-Y blit, no `glReadPixels`
stall. The GL half is `gl::TapReadback` (`alloy/src/gl/tap.rs`), the
entries `raster/window_tap.rs`, the UI side `Context::open_window_tap`
(one RPC, replying the resolved size), `set_window_tap_rect` and
`close_window_tap`; a detach completes the frame in flight first, so the
clip ends on the last frame drawn. A context loss keeps the tap and drops
its GL objects, remade by the next frame. The plugin re-reads a cut-out
node's box every frame (`frame::on_advance`, registered at the first
node recording) and moves the rect when it changed, one frame behind the
layout, with an empty rect while the node has no box. The downscale is
what makes a tablet's software VP9 keep up: the screencast is encoded at
720p or 540p, chosen by the app.

## Order

1. Encoders in forge (`Vp9Encoder`, `OpusEncoder`), round-trip tests
   through the existing decoders. Done 2026-10-16.
2. WebM writer, read-back and seek tests through `WebmDemuxer`. Done
   2026-10-16.
3. Recorder, `record()` in `flux:video`, types, capability, a headless
   `sol test` on the camera image and microphone clip, a small example
   app (`examples/record`). Done 2026-10-16, verified on the laptop
   webcam (see Findings).
4. The window tap with the node source and the cut-out, a screencast to a
   file, the readback cost measured. Built 2026-10-16 (see Findings for
   the numbers).
5. Android: the MediaCodec encoder behind `VideoEncoder`, realtime measured
   on the Pixel and the tablet. Built 2026-10-16: the Pixel encodes in
   hardware (see Findings); the tablet has no VP9 encoder and `record`
   rejects there with `unsupported`, by decision (below).
6. The live form, the transport and the live player from a JS stream
   (with [[video-streaming]] mode 2); the screencast is the first
   two-device demo, one way, then the chat adds the second direction and
   the microphone. Built 2026-10-16: `record` without `to`, `open` of a
   `ReadableStream` with `latency`, `examples/screencast` over a TCP
   connection on the local network (`flux:net`; see Findings for why not
   `flux:p2p`), laptop to tablet, Pixel to laptop and Pixel to tablet
   verified.

## Open

- **Devices without a VP9 encoder** (decided 2026-10-16: left
  `unsupported`, to be revisited when a need arises). The SM-T500 is one
  (see Findings): its hardware encoders are AVC and HEVC, the platform's
  software VP9 encoder is excluded on slow-cpu devices, and its software
  VP8 (`OMX.google.vp8.encoder`, libvpx, declared 21-30 fps at 720p) is
  the only royalty-free encoder it has. The two ways to serve such a
  device, when wanted: libvpx's own VP9 encoder built for Android too
  (+1.3 MB, the same library at our realtime settings; MediaCodec stays
  first where it has VP9), or VP8 as a second codec through the whole
  pipe (writer and reader ids, the VP8 decoder on in libvpx, the VP8 mime
  in the MediaCodec paths, codec selection in the players, the live form)
  for roughly twice the software speed. The Pixel 7's
  `c2.exynos.vp9.encoder` is hardware, up to 3840x2176, declared 160-240
  fps at 720p.
- **Pixel format into the encoder.** The tap holds upright RGBA, so each
  frame is converted to I420 on the recorder thread. Cheap at 640x480; at
  720p30 measure it before deciding whether the tap should also expose the
  camera's native YUV.
- **Transport and framing** (step 6 took WebM over one reliable, ordered
  byte stream: the mode-2 reader as is, a TCP connection or a p2p stream
  alike; a lost packet stalls everything behind it, which the live
  catch-up then cuts). Still open: `flux:p2p` for a phone whose direct
  UDP drops batched sends (a self-hosted relay, never a public one), and
  QUIC datagrams with keyframe requests if the stall-and-cut shows.

## Not in scope

- **Echo cancellation.** A chat on speakers feeds the far end's voice back
  into the microphone. SDL does none; it needs its own item (an AEC over
  the ring and the output mix, e.g. the WebRTC audio processing module).
- Signalling, rooms, more than two peers, NAT details beyond what
  `flux:p2p` and its relay already do.
- H.264/AV1 (royalty and size, see [[video-playback]]); VP8.
- `sol render` to video (`okf/ideas.md`): the file form over the window
  tap covers it once both exist; not wired in this item.

## Done looks like

- A clip recorded from camera and microphone to a WebM file plays back in
  the existing player, in sync, and seeks.
- A screencast of the window recorded the same way, with the readback cost
  per frame measured on the laptop and the encode cost on the Pixel.
- `video-encode` is a feature at every layer; `Flux.capabilities` reports
  it; a build without it rejects `record` with a plain error.
- forge tests: encode a synthetic sequence, decode it with the existing
  decoder, compare; Opus round trip likewise; the writer's output reads
  back through `webm.rs`, seeks through its Cues, and the live form reads
  as unbounded.
- Two devices in a screencast (done 2026-10-16, over TCP on the local
  network) and then a video call,
  glass-to-glass latency measured and stated, a dropped connection
  recovering on a keyframe (step 6).

## Findings

- **The pause is two times, not a flag.** A first version measured the
  gap from the last input taken to the first after the resume, which also
  swallowed one frame interval and made the decision depend on the order a
  frame and the resume crossed the channel in. `pause(at_ns)` and
  `resume(at_ns)` on the feeders' timeline (SDL ticks, from
  `sdl_utils::ticks_ns`) make it exact: an input stamped inside the pause
  is dropped however late it arrives, and the excluded time is the
  difference of the two stamps.
- **Unstamped frames take the nominal step.** The test camera (and any
  source without capture timestamps) stamps every frame 0; the recorder
  gives a frame stamped no later than the last one `last + 1/frameRate`,
  so such a clip plays at its nominal rate instead of stacking frames on
  one time.
- **Audio is timed by its samples, not its stamps.** The first block's
  stamp anchors the track; every packet after it is `emitted / rate`
  later, so Opus frames stay contiguous whatever the blocks' sizes, and a
  pause (the recorder drops the samples inside it) leaves no gap.
- **The feeder hands the tap's frame through, no copy.** The recorder's
  video input is a boxed `AsRef<[u8]>`, so the plugin passes the
  `Arc<CameraFrame>` itself and the pixels stay in the tap's buffer, shared
  with the raster upload. The tap allocates the next frame only while the
  recorder still holds this one (one encode, a few ms, against a camera
  interval of 40 ms), and its two spare buffers cover exactly that case,
  so the steady state allocates nothing per frame on either side; the
  I420 scratch is reused too. A first version copied 1.2 MB per frame.
- **`flux:camera` `open()` outside `mount` hung under the stepped test
  host**: its promise settled from the per-frame tick, and a test body
  awaiting the open drives no frame. Fixed 2026-10-16 the way the
  microphone's open already worked: the pump task settles the promise
  itself with the status the session left Pending on, and the tick settles
  whatever it reaches first. A camera test covers the outside-mount open.
- **Test traps.** The test microphone must be installed before `mount` for
  the app's open to find it; a clip installed later reaches an open
  session, and only samples pushed after `record()` started are heard (the
  tap's cursor starts at the present). The test microphone's clip arrives
  at once, not paced. The still camera stamps frames 0 and publishes one
  frame per install.
- **Laptop webcam, 2026-10-16** (i5-1340P, release client, 640x480 UVC
  camera at about 24 fps): the recorder thread spends 2.8 ms per frame on
  the RGBA to I420 conversion plus the VP9 encode (cpu-used 7, 4 threads,
  2 tile columns), no camera frame dropped over 10 s clips, the process at
  about a fifth of a core more than idle, and the UI thread unaffected
  (p95 0.5 ms, no slow frames in the `/stats` windows during three
  recordings, their starts included). A 12 s clip with Opus mono is
  1.2 MB, 770 kbit/s against the 920 kbit/s default budget. One window of
  the very first recording in the process showed 16 paints of 50 ms on
  the status text's changes; three later recordings showed none, so it
  is not understood and not reproduced.
- **The recorded file reads in ffprobe and decodes whole in ffmpeg**
  (VP9 640x480, Opus 48 kHz mono, Duration and Cues present), so the
  writer's output is not only what our reader accepts.
- **CI** runs the forge tests with `video-encode` (the encoder, writer and
  recorder tests) and the workspace tests with `flux/video-encode`.
- **The window tap on the laptop, 2026-10-16** (i5-1340P, release client,
  Mesa, 1280x720 window at scale 1, the camera viewfinder streaming so
  every refresh draws): a 19.5 s screencast at the window's size took
  1172 frames, 60 per second, with the raster thread spending 0.18 ms per
  frame issuing (the blit and the pack-buffer read) and 0.71 ms completing
  (the fence check, the row-reversing copy out of the mapped buffer, the
  publish), and the recorder 4.5 ms per 720p frame (RGBA to I420 plus VP9).
  Scaled to 640x360 it is 0.29 ms issue, 0.47 ms map and 2.3 ms per frame
  in the recorder. No frame dropped, no missed present, no slow frame, GPU
  frame time 0.6 ms, UI p95 0.6 ms during the recording. The clips (2.8
  Mbit/s at 720p, 0.7 at 360p) decode whole in ffmpeg and a frame read
  back from one is upright, the stats overlay included. The feeder misses
  the very last frame at the stop (the stop flag and the close race its
  wait), one frame per clip; left as is.
- **Pixel 7, 2026-10-16** (Tensor G2, Android 17, release client, the
  MediaCodec encoder `c2.exynos.vp9.encoder` in hardware, planar I420
  input accepted at the first try): a screencast of the 411x914-point
  window at 720 lines (324x720 pixels) ran 2268 frames in 37.7 s, 60 per
  second, at 6.5 ms per frame on the recorder thread (the I420 convert
  plus the codec's input copy and the output drain; the codec itself runs
  on its own threads), 162x360 at 5.1 ms, and the 480x640 camera with the
  microphone at 5.5 ms: the cost is mostly per call, not per pixel. No
  frame dropped, no missed present in the window, UI p95 3.2 ms, GPU
  frame time 5.3 ms. Every clip decodes whole in ffmpeg (the hardware
  clips are small: 240 kbit/s at 324x720, well under the budget), a
  screencast frame read back is upright, and the camera clip carries
  Opus at 48 kHz. The encoder's output pts equal its input pts, one
  block per frame; no superframes seen.
- **SM-T500 has no VP9 encoder at all** (Android 12, found 2026-10-16):
  `AMediaCodec_createEncoderByType` returns none. The platform's
  `c2.android.vp9.encoder` is declared `variant="!slow-cpu"` in the
  swcodec apex and the tablet is a slow-cpu device: its vendor list
  includes `media_codecs_google_video_le.xml`, whose encoders are H.263,
  H.264, MPEG-4 and VP8 only. The earlier reading of the XML (Open, above)
  missed the variant. Decided the same day: left `unsupported` there,
  the options recorded under Open.
- **A hardware encoder's output buffers carry padding after the frame**
  (Pixel 7, `c2.exynos.vp9.encoder`, found 2026-10-16 on the first live
  cast): zeros mostly, a run of 0xff on some frames (sixty of a 223-byte
  frame). libvpx finds a frame's end where its arithmetic decoder stopped,
  skips zero padding, and takes a nonzero run for a second frame with an
  invalid marker, after which every frame until a keyframe is refused;
  ffmpeg's VP9 decoder ignores trailing bytes, which hid it from the file
  checks. Our demuxer was blameless (ffmpeg's extracts the same block). The
  fix is on our decoder's input: a block without a superframe index is
  handed to libvpx as a one-frame superframe stating its length (VP9 Annex
  B), so it decodes exactly the block. The first second of such a stream
  is a fixture (`forge/src/tests/data/pixel_live.webm`) with a test
  decoding all sixty frames.
- **Laptop to tablet over p2p, 2026-10-16**: the laptop's window (1280x720
  at 720 lines, libvpx, live mode) cast to the SM-T500 over a local-only
  IPv4 direct path, the tablet playing it through the texture player
  (MediaCodec decode) at 59 fps with 12 missed presents per 10 s window
  and no buffering episode; the delay read off the caster's clock in the
  tablet's picture is about 0.8 s, which is the live resume floor (0.75 s)
  plus the pipe, at a `latency` of 0.3 s. The sender spends 37 KB/s. A
  debug command on each role (`ticket` on the shower, `cast <ticket>` on
  the caster) drives a cast without the camera.
- **The Pixel 7 as a caster stalls over `flux:p2p`'s direct UDP path**:
  its first packets arrive (the container header, 140 bytes), then
  nothing, while its writes keep succeeding into QUIC's send buffer; the
  receiver's read is never woken because no data reaches it. Over the
  earlier IPv6 or relay-mixed paths the data came late and in bursts (the
  catch-up log cut 334 ms every second, one chunk 15 s late). quinn
  batches UDP sends (GSO) whenever the kernel takes the socket option and
  backs off only on an explicit send error, so a driver dropping the
  batches silently is invisible to it, and neither iroh nor quinn-udp
  exposes a switch; an older note records this phone rejecting ECN/GSO
  sends. The laptop and the tablet as senders have no such trouble.
  Decided 2026-10-16: the screencast example runs over a TCP connection
  (`flux:net`, the receiver takes any byte stream), the QR carrying the
  shower's local address; `flux:p2p` stays for a later item (a self-hosted
  relay is the way to carry this phone, no public relay).
- **Over TCP, 2026-10-16**: Pixel to laptop and Pixel to tablet both play
  at 60 fps, about 0.8 s behind the caster's clock (the live resume floor),
  the tablet with 1 missed present per 10 s and no buffering, the laptop
  with 8; the Pixel's hardware encoder at the default budget for 324x720
  (about 240 kbit/s) gives a soft picture, so the live default bitrate is
  worth raising. Open: a reload of the caster app while it cast left the
  Pixel's old engine stuck in its teardown once (the swap never built a
  frame; a force-stop recovered it); the same scene headless on the laptop
  (a TCP loopback cast left open at the test's end) tears down cleanly,
  so the device-side cause (the MediaCodec encoder's flush, the window
  tap, the camera) is not pinned down.
- **A stepped test records nothing from a window at rest**: `frame()` of
  the test host steps app time without demanding a draw, so the screen
  tests move a box every frame. The recording's start demands one frame
  of its own, so a real screencast begins with the screen as it is.
