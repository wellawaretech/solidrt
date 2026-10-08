---
title: Native modules cannot read camera frames
description: alloy's camera pump holds every frame as upright RGBA on the CPU and hands it only to the texture upload and the QR decoder, so a custom runtime module (inference, hand tracking, OCR) can reach frames only by reading the camera texture back from the GPU; a latest-frame tap on the session, keyed by the texture id JS already holds, gives a module frames on its own thread, and QR decoding moves onto it off the UI thread.
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
  frames queued ahead of it.
- Not open the camera itself: alloy owns the SDL camera subsystem init (its
  refcounting is not thread-safe, and `ensure_init` documents how a backend
  can wedge), and the device would be held twice.

The QR decoder has the same problem from the other side: it runs inside
`pump_frame` on the UI thread, around a millisecond per attempt and several
when a code is present (desktop release), about three times a second.

## Shape

### The tap (alloy, native types only)

```rust
/// One upright camera frame: RGBA8, tightly packed rows top to bottom, the
/// layout `read_texture` and `barcode::scan_rgba` use.
pub struct CameraFrame {
  /// Increases by one per frame the session pumped; a gap is a dropped frame.
  pub seq: u64,
  /// SDL's capture timestamp.
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
  `createCamera`), it stays the same when the device rotates (the pump
  recreates the texture at the same id), and the session id stays out of JS
  as the plugin intends ("the raw handle never crosses into JS").
- **Latest wins.** The slot holds one frame. A consumer slower than the
  camera misses frames instead of queueing them; `seq` tells it how many.
  The same rule as the wake-word worker's realtime catch-up.
- **No cost unless held.** The session owns the shared slot; `pump_frame`
  publishes only while a tap exists besides its own (`Arc::strong_count`).
  Then it costs one copy of the upright frame per pumped frame, 1.2 MB at
  640x480. The previous frame's buffer is reused when no consumer still
  holds it, so the steady state does not allocate.
- **Timestamp.** `sdl_utils::camera_acquire_frame` passes null for SDL's
  timestamp today; the tap needs it, since trackers smooth and estimate
  velocity over capture time, not arrival time.
- **Close wakes everyone.** `close_camera` is the one close path (an app's
  `close()`, and engine teardown through the camera plugin's `Drop`). It
  marks the slot closed and wakes every waiter with `Closed`, so a module's
  worker ends with its engine and never holds up a reload.
- **Upright, not mirrored, not scaled.** The frame is what the viewfinder
  shows. Mirroring a front camera is the consumer's call. No downscale:
  consumers want different sizes (a detector a letterboxed square, a
  landmark model a full-resolution crop around the hand), inference
  libraries convert and resize from RGBA in one pass on their own thread
  (ncnn's `from_pixels_resize`), and a resize in the pump would put that
  work on the UI thread. The open size (`width`/`height`) already sets the
  resolution.
- **Pacing.** Frames arrive at the pump's rate, the frame loop, and a
  streaming camera already demands frames.

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

A session opened with `scan: ["qr"]` takes a tap and decodes on its own
thread instead of inside `pump_frame`: wait for a newer frame, convert to
grey, decode, sleep out the interval (time-based, about three a second as
now), push payloads into a shared list that `take_camera_barcodes` drains.
The one-second re-report throttle stays, and the JS API does not change.
`Session` loses `gray`, `frames_since_scan` and `last_emit`.

This takes the decode off the UI thread, and it gives the tap an in-tree
consumer from the start, exercised by every QR scan rather than only by
custom runtimes.

## Rejected

- **Texture readback per frame.** Today's workaround: a CPU to GPU to CPU
  round trip of pixels the pump already had, and the JS thread waits on the
  raster thread for each read.
- **A per-frame callback with the pixel slice.** Zero-copy, but it runs the
  consumer's code on the UI thread inside the pump; a slow model stalls the
  frame.
- **A channel of every frame.** A consumer slower than the camera builds a
  backlog and its latency grows without bound.
- **The module opens the camera itself.** See Symptom.
- **Scaling or converting in the tap.** See "Upright, not mirrored, not
  scaled".
- **A JS pixel API.** QuickJS is too slow to touch every pixel of every
  frame. A `cam.snapshot()` (deferred in the camera design) can come later
  over the tap; modules do not need it.

## Done looks like

- A module in a custom runtime reads upright RGBA frames of an open camera
  on its own thread, newest frame wins, and its worker ends when the camera
  closes or the engine reloads.
- No cost while no tap is held; one copy per frame while one is.
- QR scanning runs on the tap, off the UI thread, and behaves the same from
  JS, checked with a real camera on Linux and on the Android tablet.
- The "Native code" section of `docs/runtime/index.md` names the tap.

## Order

1. The tap in alloy: types, publish in `pump_frame`, buffer reuse, SDL
   timestamp, close semantics. Unit tests in `alloy/src/tests/` for the
   slot on its own (publish, latest, wait newer, timeout, close wakes
   waiters, buffer reuse); no camera needed.
2. QR onto the tap. Verified by hand with a real camera on Linux and on the
   tablet: there is no synthetic camera source.
3. Docs.
4. Measure the copy on the tablet at 640x480 and 1280x720, release build.

## Open

- The key: the texture id (proposed) or a handle object the camera plugin
  hands JS. The texture id needs no new JS surface; a handle would be
  explicit but is one more object for every consumer to pass around.
- No synthetic camera exists, so QR and every tap consumer are verified by
  hand. A synthetic source (an image or a video file standing in for a
  device) would make them testable under `sol test`; its own item if wanted.

## Not in scope

- The inference engine and hand tracking themselves: custom-runtime work
  over the tap.
- GPU-side downscale, mirroring, pixel formats other than RGBA8.
