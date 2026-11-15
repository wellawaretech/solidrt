---
title: Encode video and audio
description: The runtime decodes VP9 and Opus but cannot encode either, so an app cannot record its camera and microphone to a file or send them anywhere; a VP9 encoder (libvpx off Android, MediaCodec on it), an Opus encoder and a WebM writer in forge, fed natively from the camera tap and the microphone ring, surfaced as `record()` in flux:video; recording to a file first, the window as a second source, video chat over p2p as the later consumer of the same pipe.
created: 2026-10-16
completed: 2026-11-14
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
- **MediaCodec** is bound in `forge/src/video/mediacodec/` for decoding;
  an encoder is the same API in the other direction
  (`MediaCodec::from_encoder_type` exists in ndk 0.9).
- **The WebM reader** (`forge/src/video/webm.rs`) is ours and already
  accepts an unknown-size Segment of unknown-size Clusters (the live form);
  there is no writer.
- **The camera tap** (`alloy::camera::FrameTap`) publishes upright RGBA
  frames with SDL tick timestamps; **the microphone ring**
  (`alloy::microphone::MicrophoneTap`) is read by cursor from any thread
  and its `opened_ns` is on the same timeline, so sync costs nothing.
- **Live playback** (mode 2) was designed but deferred in
  [[video-streaming]]; the receiving end of a chat or screencast needs it,
  and step 6 built it.

## Shape

Layering by the usual question: a flux CLI with no window could encode a
file's frames, so the encoders and the writer are forge. The taps are
alloy. They meet on plain values (an RGBA frame with its timestamp, a block
of PCM with the time of its first sample): the recorder pulls them through
its input contracts (`VideoInput`, `AudioInput`) with a pump thread each,
and the plugin adapts the taps to those contracts in one line each, as
playback adapts alloy's sinks to `FrameSink` and `AudioSink`.

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
  block. `attach(Inputs)` starts the pumps over live sources: the video
  pump takes every frame published past the last it took (and the one in
  flight at the stop), the audio pump forwards blocks of one Opus frame;
  `Feeder` stays for a producer that must lose nothing. `stop` ends the
  pumps, joins and finalizes.

### The pipe and the plugin

`flux/src/alloy_plugins/record.rs` exports `record`: `TapInput` adapts
`FrameTap::wait_newer`, `MicInput` adapts `MicrophoneTap::wait` and
`read_into` (whose `MicRead::timestamp_ns` places the block on the SDL
ticks), and the recorder attaches both. A cut-out's box in device pixels
is the rendertree's `device_box` (the pixels the paint covers). Engine
teardown stops every open recording with a bounded join and the writer
closes the file; nothing outlives the engine. Types in
`packages/flux-types/gui/video.d.ts`, `"video-encode"` in the gui
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
7. The self-hosted relay: `serveRelay` in `flux:p2p` over iroh-relay's
   server half, behind the `p2p-relay` feature in every stock runtime,
   plain HTTP on the local network, its `url` what peers pass as
   `relayUrl`; the path for a pair with no working direct UDP path. The
   Pixel's own stall is a bug in the pinned noq-udp (see Findings), not
   what the relay is for. Built 2026-10-17: `serveRelay` with its token,
   flux and forge tests, the size row.
8. The call: `examples/call`, host and guest over `flux:p2p` with the host
   serving the relay, camera and microphone both ways, each side playing
   the other's at the live latency, hold, far side off, hang up, a redial
   after a drop; a two-way loopback test through a relay in the record
   suite. Built 2026-10-17; laptop host to tablet guest verified one way
   (the tablet has no VP9 encoder and joins receiving only, by the
   decision above); the two-way run and the latency read followed on the
   Pixel (Findings); the live bitrate is a follow-up.

## Follow-ups

Closed 2026-11-14. Open work left by this item:

- The Pixel's hardware encoder runs at a third of the bitrate asked, a
  soft picture: [[mediacodec-vp9-bitrate-undershoot]].
- Keyframe on demand, a bitrate that follows the link, and QUIC
  datagrams if the stall-and-cut shows: [[live-video-link-adaptation]].
- Two peers behind different NATs stay on the relay (no QUIC address
  discovery), and whether `sol run` serves a relay is undecided:
  [[p2p-relay-across-nats]].
- A reload while casting left the Pixel's old engine stuck in teardown
  once: [[android-caster-reload-teardown]].
- The stepped host deadlocks a live player against a blocking raster
  call: [[headless-live-take-deadlock]].
- The call's redial after a network drop is covered by the code path but
  was never read on a device: a line in okf/tiny.md (Runtime).

Settled while open, kept here for the trail:

- **Devices without a VP9 encoder** stay `unsupported` (decided
  2026-10-16), a documented limitation with the two ways to serve them in
  okf/notes/android-vp9-encoders.md.
- **Pixel format into the encoder.** The RGBA to I420 conversion on the
  recorder thread is cheap enough at 640x480 and 720p
  (okf/notes/video-encode-measurements.md); the tap does not expose the
  camera's native YUV.
- **The call's delay.** About a second at first (2026-10-17): the live
  resume floor of the time (the stream low water plus 250 ms, 0.75 s)
  plus the pipe, which a `latency` of 0.3 s could not go below. The floor
  is 0.15 s since 2026-10-20 ([[live-latency-floor]]: the live thresholds
  follow the `latency` option, a stall's backlog is skipped back to the
  latency); the call runs at it on the laptop and the Pixel over the
  relay with no buffering episode, no skipped or late frame and no cut
  over a minute each way, 0.2 to 0.3 s glass to glass.
- **The Pixel as a sender over a direct path** ran 2026-11-03 after the
  iroh 1.4.1 bump (okf/upstream/noq-udp-android-eio-ecn-cmsg-send-stall.md,
  resolved): live both ways on `path mixed`. A fresh endpoint's first
  call stalled 15 s until forge built Android endpoints without GSO
  (okf/upstream/noq-udp-android-gso-eio-first-call-stall.md); with that,
  the first call is live by +4 s and `paths: "relay"` is no longer needed
  for this phone.

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
  glass-to-glass latency measured and stated (0.2 to 0.3 s), a dropped
  connection recovering on a keyframe (the code path is covered; the read
  on a device is a follow-up).

## Findings

The measurements are in okf/notes/video-encode-measurements.md, the
Android encoder facts (the SM-T500 has none, the Pixel's pads its output)
in okf/notes/android-vp9-encoders.md, and the recording test traps in
okf/notes/headless-verification-traps.md. What follows is how the work
went.

- **The pause is two times, not a flag.** A first version measured the
  gap from the last input taken to the first after the resume, which also
  swallowed one frame interval and made the decision depend on the order a
  frame and the resume crossed the channel in. `pause(at_ns)` and
  `resume(at_ns)` on the feeders' timeline (SDL ticks, from
  `ore::time::ticks_ns`) make it exact: an input stamped inside the pause
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
- **The pump hands the tap's frame through, no copy.** The recorder's
  video input is a boxed `AsRef<[u8]>`, so the plugin's adapter passes the
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
- **CI** runs the forge tests with `video-encode` (the encoder, writer and
  recorder tests) and the workspace tests with `flux/video-encode`.
- **Laptop to tablet over p2p, 2026-10-16**: the laptop's window (1280x720
  at 720 lines, libvpx, live mode) cast to the SM-T500 over a local-only
  IPv4 direct path, the tablet playing it through the texture player
  (MediaCodec decode) with no buffering episode, about 0.8 s behind: the
  live resume floor of the time (0.75 s) plus the pipe, at a `latency` of
  0.3 s (the figures in okf/notes/video-encode-measurements.md). A
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
  at 60 fps, about 0.8 s behind the caster's clock (the live resume
  floor). The Pixel's hardware encoder at the default budget for 324x720
  (about 240 kbit/s) gives a soft picture
  ([[mediacodec-vp9-bitrate-undershoot]]). A reload of the caster app
  while it cast left the Pixel's old engine stuck in its teardown once
  ([[android-caster-reload-teardown]]).
- **The Pixel's upload stall is a noq-udp bug, not the direct path** (found
  2026-10-17, from the phone's `sendmsg error ... EIO ... ecn: Ect0,
  segment_size` line): the pinned noq-udp halts GSO on an EIO but drops the
  ECN cmsg only on EINVAL, and this kernel answers the cmsg with EIO too,
  so every later send fails while receives work. Fixed upstream in quinn
  (PR 2719, 2026-07-10), synced into noq's main 2026-10-16, no release
  yet: okf/upstream/noq-udp-android-eio-ecn-cmsg-send-stall.md. The
  phone's first p2p call (2026-10-17, a direct IPv4 path plus the relay)
  showed the bug's full shape: both opens timed out, the phone's own
  included, since its ACKs leave no more than its data, and `connInfo`
  went from `mixed` to `relay` only after about a minute, while the phone
  kept reporting the direct path active (packets still reach it) and its
  pipe writes kept succeeding into the send buffer, so neither a hang-up
  nor a drop reached its app. Hence `paths: "relay"` on `Endpoint.create`
  (added the same day, default `"any"`): no UDP socket at all, every
  packet through the relay, the one way to use this phone until the noq
  bump; the call demo takes it through `call_debug relayOnly true`.
- **The self-hosted relay (step 7), 2026-10-17.** iroh-relay's server half
  over plain HTTP works as is: iroh exchanges relay URLs with the http
  scheme and dials ws, and its net report's HTTP probe takes the URL as
  given. What cost a round: `RelayMode::custom(urls)` writes the default
  QUIC address-discovery port into each relay entry, so the report probes
  that port on our HTTP-only relay and waits the probes out before the
  HTTP latency counts, and the endpoint was online (its ticket carrying
  the relay) only after 3 s; built as `RelayConfig::new(url, None)` it is
  online in 0.2 s. Without address discovery the endpoint learns no public
  address from the relay: on one network it is moot
  ([[p2p-relay-across-nats]] for two). The relay admits its own token only
  (a fresh 32-byte one per relay, checked on the websocket upgrade's
  bearer header through iroh-relay's access hook), carried as the user
  part of the relay URL (`http://<token>@host:port/`): an endpoint strips
  it into its relay config and its ticket writes it back, so the guest
  gets it from the QR, and a port reachable from further away is not an
  open relay. The size row is in okf/notes/video-encode-measurements.md.
- **The call on devices, 2026-10-17** (laptop host, SM-T500 guest, the
  laptop serving the relay, the tablet dialing the ticket through
  `call_debug call <ticket>`): the tablet has no VP9 encoder, so its
  `record` rejects and it joins receiving only, which the demo now allows
  (a side whose recording cannot start finishes its send half, since a
  QUIC stream reaches the acceptor only with the dialer's first bytes,
  and stays in the call; a far open that fails is a peer sending nothing
  and keeps the call; a far picture failing after it played is the drop
  that ends it). The tablet played the laptop's camera live with no
  missed present over a 5 s window, `connInfo` reporting `mixed` (the
  relay and the direct path both active) for the call's length; a hang
  up on the host returned the guest to its scanner and a second dial
  connected again; Wi-Fi off on the tablet had the host report the lost
  connection within 45 s. The player's `finished` and `buffering` are
  plain reads, so a status text over them must poll them into signals,
  which the screencast's player does not (a line in okf/tiny.md).
- **The record suite deadlocked the headless host one run in four** with
  the two-way call test, by a hazard older than the test: under the
  stepped clock a playing live player's frame take waits for a frame due
  after the deadline, that frame arrives through the JS pump on the UI
  thread, and the UI thread was blocked in a texture-create RPC for the
  second player, queued behind the frame on the raster thread (gdb as the
  client's parent, since ptrace is otherwise refused here: `sol-raster`
  in `YuvLatchShared::take`, `sol-ui` in `create_texture_at`'s `recv`).
  The test now opens both players before it steps a frame; the hazard is
  [[headless-live-take-deadlock]]. The feeder's missed frame at the stop
  is gone with the drain (the window clips take 30 of 30 frames; the
  live ones lose one to the full channel instead).
- **The two-way call, laptop and Pixel 7, 2026-10-17**, the phone
  relay-only (`call_debug relayOnly true`, then `call <ticket>`), the
  laptop serving the relay and dialled by the phone: both sides live and
  sending within 25 s, `connInfo` `relay` on both, the laptop's far
  picture the phone's 480x640 camera and the phone's the laptop's, read
  as pixels (over a thousand distinct colours in a 48x32 crop). Over a
  10 s window the laptop latched 2460 frames in all, 0 skipped, 0 late,
  0 missed presents; the phone 1527, 0 skipped, 0 late, 4 missed
  presents. The phone's first, direct-path attempt had its recording drop
  2365 of 2739 frames at the full live sink (the stall above); the
  relay-only one encodes at the step 5 rate. The laptop's camera list
  went stale when a second camera appeared mid-session (the open named a
  `/dev/video2` that had moved), cured by a fresh client, so a camera
  that fails to open after a hotplug is the SDL list, not the app.
