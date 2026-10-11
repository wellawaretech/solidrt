---
title: Native modules cannot read camera frames
description: alloy's camera pump holds every frame as upright RGBA on the CPU and hands it only to the texture upload and the QR decoder, so a custom runtime module (inference, hand tracking, OCR) can reach frames only by reading the camera texture back from the GPU; a camera thread fills a latest-frame tap keyed by the texture id JS already holds, the raster upload, the QR decoder and any module read from it, and the UI thread does no per-frame pixel work.
created: 2026-10-08
---

# Native modules cannot read camera frames

## Symptom

A custom runtime module that works on camera images (object detection, hand
landmarks, OCR, motion) needs each frame as CPU pixels on a thread of its
own. The pixels exist: `pump_frame` in `alloy/src/camera.rs` has the upright
RGBA8 frame on the CPU every frame (SDL's surface, or the session's
`convert`/`scratch` buffers), uploads it into the stream texture, and every
tenth frame runs the QR decoder on it. Nothing hands them out. `Session` is
private, and `Context` exposes only open, status, barcodes and close.

What a module can do today:

- Read the camera texture back with `read_texture_by_id` each frame. Pixels
  the pump had on the CPU go to the GPU and back, at full resolution, and
  the read is an RPC to the raster thread, so the JS thread waits behind the
  frames queued ahead of it. The read cannot move to a worker either:
  `alloy::Context` is not `Sync` (its registries, the camera's among them,
  are `RefCell`s), so it runs on the JS thread.
- Not open the camera itself: alloy owns the SDL camera subsystem init (its
  refcounting is not thread-safe, and `ensure_init` documents how a backend
  can wedge), and the device would be held twice.

What the readback costs, measured 2026-10-11 with an inference module in a
custom runtime that reads the camera texture back once per inference
(release builds):

- Desktop (i5-1340P, Mesa Intel, a 640x480 UVC webcam), about ten reads a
  second: a frame that starts a read spends up to 10.8 ms in JS, against
  about 2 ms for the others (p50 2.2 ms, p95 10 ms). The raster thread
  spends 64 ms of every second on side work, the camera uploads plus the
  reads.
- Pixel 7 (packed APK), about 21 reads and inferences a second, 23 ms
  each on the module's thread: no frame over budget once running, and
  presents follow the camera with no gap of four refreshes or more.

So the readback is a cost, not yet a visible problem: the tap removes a
round trip and a JS-thread stall rather than a jank anyone sees today.

The QR decoder has the same problem from the other side: it runs inside
`pump_frame` on the UI thread, around a millisecond per attempt and several
when a code is present (desktop release), about three times a second.

## Shape

The camera gets one output, a latest-wins slot its own thread fills, and
everything that wants frames reads that slot: the raster thread for the
texture, the QR decoder, a module. This mirrors the video path, where a
worker publishes into a latch the raster thread takes from at each frame
(`yuv.rs`), and it takes the per-frame convert, rotate and copy off the UI
thread, where the pump does them today.

### The tap (alloy, native types only)

```rust
/// One upright camera frame: RGBA8, tightly packed rows top to bottom, the
/// layout `read_texture` and `barcode::scan_rgba` use.
pub struct CameraFrame {
  /// Increases by one per frame the camera produced; a gap is a dropped frame.
  pub seq: u64,
  /// SDL's capture timestamp, on the SDL tick timeline (`sdl_utils::ticks_epoch`).
  pub timestamp_ns: u64,
  pub width: u32,
  pub height: u32,
  pub pixels: Vec<u8>,
}

pub enum TapWait {
  Frame(Arc<CameraFrame>),
  Timeout,
  /// The session closed; no frame will follow.
  Closed,
}

#[derive(Clone)]
pub struct FrameTap(Arc<TapShared>);

impl FrameTap {
  /// The newest frame, without waiting.
  pub fn latest(&self) -> Option<Arc<CameraFrame>>;
  /// Block until a frame newer than `after` arrives, the session closes, or
  /// `timeout` passes.
  pub fn wait_newer(&self, after: u64, timeout: Duration) -> TapWait;
}

impl Context {
  /// A tap on the open camera streaming into `texture_id`; None when no open
  /// session owns that id.
  pub fn camera_frame_tap(&self, texture_id: u64) -> Option<FrameTap>;
}
```

- **Keyed by the stream texture id.** It is the one identity JS holds for a
  camera (`cam.texture` from `flux:camera` `open()`, `texture()` from
  `createCamera`), it stays the same when the device rotates (the texture
  is recreated at the same id), and the session id stays out of JS as the
  plugin intends ("the raw handle never crosses into JS").
- **Latest wins.** The slot holds one frame. A consumer slower than the
  camera misses frames instead of queueing them; `seq` tells it how many.
  The same rule as the wake-word worker's realtime catch-up.
- **Always filled, buffers reused.** The slot is the camera's output, not
  a side channel, so it is filled whether or not anyone besides the
  texture reads it. The previous frame's buffer is reused for the next
  frame when no consumer still holds it, so the steady state does not
  allocate; a consumer holding a frame across a publish costs one
  allocation, not one per frame.
- **Timestamp.** SDL's per-frame capture timestamp, which trackers smooth
  and estimate velocity over, rather than arrival time.
- **Close wakes everyone.** `close_camera` is the one close path (an app's
  `close()`, and engine teardown through the camera plugin's `Drop`). It
  marks the slot closed and wakes every waiter with `Closed`, so a module's
  worker ends with its engine and never holds up a reload.
- **Upright, not mirrored, not scaled.** The frame is what the viewfinder
  shows, so it carries no rotation and a result (a box, a landmark) maps
  onto the preview as is. Mirroring a front camera is the consumer's call.
  No downscale, although analysis usually wants a frame smaller than the
  preview (a detector reads 320 to 640 pixels): consumers want different
  sizes (a detector a letterboxed square, a landmark model a
  full-resolution crop around the hand), inference libraries convert and
  resize from RGBA in one pass on their own thread (ncnn's
  `from_pixels_resize`), and a resize in the producer would delay every
  frame for one consumer's size. The open size (`width`/`height`) already
  sets the resolution.

### The camera thread

Once the permission resolves, the session hands its SDL device to a
`sol-camera` thread, which owns it from then on: acquire the newest queued
frame (SDL documents acquire, release and close as callable from any
thread), convert to RGBA32 when the native format differs, rotate upright
per SDL's per-frame rotation into the slot's spare buffer, publish with
SDL's timestamp, release. `SDL_AcquireCameraFrame` never blocks and SDL
offers no wait, so the thread sleeps a few milliseconds between empty
polls, well under a display refresh. The thread ends when the tap closes
and closes the device on its way out; `close_camera` joins it, so the
device is free when the close returns, which the retry path relies on.

The June decision that no camera thread is needed stands corrected: it was
right that nothing blocks, and wrong about where the per-frame work
belongs.

### The raster upload

The UI side creates the stream texture at the first frame (and recreates it
at the same id when the upright size changes) and attaches the tap to it,
the way a YUV output is attached to its latch. At each `Frame` the raster
thread reads the slot and, when the seq advanced and the size matches the
texture, uploads the frame through its staging buffer. No frame crosses the
raster channel any more: the `UpdateTexture` copy on the UI thread goes
away. A frame published between the UI side's size check and the raster
take, at the instant of a rotation, is skipped once.

### The UI pump, cut down

Per frame the pump polls the deferred init and the permission answer while
Pending, starts the producer once approved, and then only reads the latest
seq and size: a new size recreates the texture and marks the session Ready,
a new seq notes content on the texture (damage) and reports a redraw. An
open camera is standing frame demand like a playing video
(`Context::streaming_textures`), so the loop ticks on the refresh grid and
the raster take sees every frame within one refresh.

### A module over the tap

The JS side passes the texture id (`hands.track(cam.texture)`); the module's
binding takes the tap and hands it to a worker:

```rust
let tap = lattice::flux::gui::alloy_context(&ctx)
  .and_then(|alloy| alloy.camera_frame_tap(texture_id))
  .ok_or_else(|| throw(&ctx, "track: not an open camera's texture"))?;
std::thread::spawn(move || {
  let mut seen = 0;
  loop {
    match tap.wait_newer(seen, WAIT_TIMEOUT) {
      TapWait::Frame(frame) => {
        seen = frame.seq;
        results.send(infer(&frame));
      }
      TapWait::Timeout => {}
      TapWait::Closed => break,
    }
  }
});
```

Results come back to JS through a channel drained in the module's
`flux::gui::frame::on_advance` tick, the shape speech and the physics
modules use. Types are named through `lattice::flux::alloy`.

### QR scanning onto the tap

A session opened with `scan: ["qr"]` spawns a `sol-qr` thread over a tap
clone: wait for a frame newer than the last seen, decode it when the
interval is due (time-based, about three a second as now), push payloads
into a shared list that `take_camera_barcodes` drains. The one-second
re-report throttle stays, and the JS API does not change. The decoder stays
off the camera thread so a slow decode on a phone never delays a published
frame.

This takes the decode off the UI thread, and it gives the tap an in-tree
consumer from the start, exercised by every QR scan rather than only by
custom runtimes.

## Rejected

- **The UI thread as the producer, the tap as a side channel** (the first
  shape of this item): keeps convert, rotate and the raster copy on the UI
  thread every frame and adds a third copy for the tap, with a "publish
  only while held" mode to keep that copy off the common path. One output
  with one producer thread needs no mode.
- **Texture readback per frame.** Today's workaround: a CPU to GPU to CPU
  round trip of pixels the pump already had, and the JS thread waits on the
  raster thread for each read.
- **A per-frame callback with the pixel slice.** Zero-copy, but it runs the
  consumer's code inside the producer; a slow model stalls the camera.
- **A channel of every frame.** A consumer slower than the camera builds a
  backlog and its latency grows without bound.
- **The module opens the camera itself.** See Symptom.
- **Scaling or converting in the tap.** See "Upright, not mirrored, not
  scaled".
- **QR decoding on the camera thread.** No second thread, but a decode
  longer than a camera frame interval (a code present, 720p, a phone)
  drops a frame three times a second.
- **A JS pixel API.** QuickJS is too slow to touch every pixel of every
  frame. A `cam.snapshot()` (deferred in the camera design) is a copy of
  `latest()` whenever wanted; modules do not need it.

## Done looks like

- A module in a custom runtime reads upright RGBA frames of an open camera
  on its own thread, newest frame wins, and its worker ends when the camera
  closes or the engine reloads.
- The UI thread does no per-frame pixel work for a camera: no convert, no
  rotate, no copy to the raster thread.
- QR scanning runs on the tap, off the UI thread, and behaves the same from
  JS, checked with a real camera on Linux and on the Android tablet, and
  under `sol test` over a still image.
- The "Native code" section of `docs/runtime/index.md` names the tap.

## Order

1. The tap in alloy: types, publish, buffer reuse, close semantics. Unit
   tests in `alloy/src/tests/` for the slot on its own (publish, latest,
   wait newer, timeout, close wakes waiters, buffer reuse); no camera
   needed.
2. The camera thread, the raster attach and take, the UI pump cut down,
   SDL timestamp.
3. QR onto the tap. Verified by hand with a real camera on Linux and on the
   tablet: there is no synthetic camera source.
4. Docs.
5. Measure on the tablet at 640x480 and 1280x720, release build: the UI
   thread's frame time with a camera open against today's, and an
   inference consumer on the tap against its readback numbers in Symptom
   (JS time on the frames that start an inference, the raster thread's
   side work).

### A still image as the camera, for tests

`Context::install_still_camera(width, height, pixels)` makes an RGBA8
image the camera: the device list is that one camera (id 0), every open
is approved at once with no SDL subsystem and its tap holds the image
from the open, and a later install publishes the new image into every
open still session. No thread runs for it and it holds no frame demand,
so an app test settles. `@solidrt/test` exposes it as `app.camera({ width,
height, pixels })` over the `sol:test` verb `camera`. The barcode path
stays the real one: the scanner thread decodes the still, the plugin holds
the engine while the scanner has an unexamined frame (`camera_scan_pending`)
and demands the frame that delivers the payload, and registering a barcode
handler demands a frame so a payload decoded before the handler existed is
delivered. `packages/core/tests/camera.test.tsx` runs the viewfinder, the
QR decode and a re-install under `sol test`.

A video file as the source is not built: a test that wants a sequence
installs the frames one by one.

## Open

- Nothing.

## Not in scope

- The inference engine and hand tracking themselves: custom-runtime work
  over the tap.
- GPU-side downscale, mirroring, pixel formats other than RGBA8.
