---
title: Native modules cannot read the microphone off the UI thread
description: alloy's microphone registry is a RefCell read on the UI thread and the JS session hides its id, so a custom runtime module (speech recognition beside a detector) gets samples only through a JS pump that keeps the app from resting; a bounded ring per session, filled from SDL's put callback, read by cursor from any thread (JS, a module's worker, the speech recognizer), keyed by a session id the app passes, with a test source under sol test.
created: 2026-10-11
completed: 2026-10-12
---

# Native modules cannot read the microphone off the UI thread

## Symptom

A custom runtime module that runs a model on microphone audio (speech
recognition with sherpa-ncnn beside an object detector, a keyword spotter,
a voice changer) wants the samples on a thread of its own, the way
`camera_frame_tap(texture_id)` hands a module the camera's frames. The
microphone has no equivalent:

- `MicrophoneRegistry` (`alloy/src/microphone.rs`) keeps its sessions in a
  `RefCell`, so `read_microphone` works on the UI thread only.
- The session `flux:microphone`'s `open()` returns keeps its id in Rust, so
  a module cannot find the session the app opened.

So audio for a model goes the long way: JS calls `read()`, hands the
samples to the module, the module hands them to its worker. The copy is
cheap (64 KB/s at 16 kHz mono); the pump is the cost. core's
`microphone.ts` says to poll `read()` "e.g. once per frame", and a session
that is not read grows without bound (the alloy module doc says so). An
`onFrame` poll keeps the app from ever resting, and a timer wakes JS on a
schedule for work that needs no JS.

The in-tree speech plugin (`lattice/src/plugins/speech.rs`) has the same
shape in a worse form: its `on_advance` tick pumps the mic into the
recognizer's worker and drains the worker's events, and the tick returns
false. On an app at rest no frame runs, so a listening session is never
pumped and no result is ever delivered.

Found while planning a speech module for the `vision` demo (feedback item
6 there), from the sources; not built.

## Shape

The session gets one output, a bounded ring of samples SDL's audio thread
fills, and everything that wants samples reads it by a cursor of its own:
the JS `read()`, a module's worker, the speech recognizer. This is the
camera tap's model with the one change audio needs: a consumer must see
every sample, so instead of a latest-wins slot the ring keeps a window,
and a consumer slower than the window loses the oldest samples and is
told how many.

### The ring (alloy, native types only)

```rust
/// What a wait on the ring ended with.
pub enum MicWait {
  /// At least the asked-for samples are buffered past the reader's cursor.
  Samples(usize),
  Timeout,
  /// The session closed; no sample will follow.
  Closed,
}

/// What one read drained.
pub struct MicRead {
  /// Position of the first sample read, in samples since the open.
  pub position: u64,
  /// Samples the reader lost to the window before this read (0 when it kept up).
  pub dropped: u64,
  /// Samples appended to `dst`.
  pub count: usize,
}

/// A reader on a session's ring, usable from any thread; its cursor is its own.
pub struct MicrophoneTap { .. }

impl MicrophoneTap {
  pub fn sample_rate(&self) -> u32;
  /// The open on the SDL tick timeline (`CameraFrame::timestamp_ns` is on it):
  /// position / sample_rate past this is a sample's time.
  pub fn opened_ns(&self) -> u64;
  /// Samples buffered past the cursor, without waiting.
  pub fn available(&self) -> usize;
  /// Block until `min_samples` are buffered past the cursor, the session
  /// closes, or `timeout` passes.
  pub fn wait(&self, min_samples: usize, timeout: Duration) -> MicWait;
  /// Drain everything buffered past the cursor into `dst`. Err once closed.
  pub fn read_into(&mut self, dst: &mut Vec<f32>) -> Result<MicRead, String>;
}

impl Context {
  /// A new reader on the session, hearing from now on; Err when unknown.
  pub fn microphone_tap(&self, sid: u64) -> Result<MicrophoneTap, String>;
}
```

- **Keyed by the session id.** The camera is keyed by its texture id
  because JS already holds that; a microphone session has nothing JS
  holds, so the id crosses: `open()` of `flux:microphone` returns it as
  `id`, core's `MicrophoneStream` exposes `id()`. The methods stay bound.
- **Bounded window, dropped count.** `RING_SECONDS` of audio at the
  session rate (10 s is 640 KB at 16 kHz mono). A reader whose cursor
  falls behind the window is moved to the oldest kept sample and its next
  read reports `dropped`. This ends the "an unread session grows" rule:
  an app at rest with a mic open loses old audio, which is right.
- **SDL's audio thread is the producer.** `SDL_SetAudioStreamPutCallback`
  fires on the stream when the recording device delivers converted data;
  SDL holds the stream lock during the callback and allows
  `SDL_GetAudioStreamData` inside it. The callback drains the stream into
  the preallocated ring under a short lock and wakes waiters. No thread of
  our own, no poll, no allocation in steady state. The camera needs a
  producer thread because SDL offers no camera wait; audio has one.
- **Readers never touch SDL.** The session on the UI thread owns the SDL
  stream; readers hold the ring (`Arc`). So the stream pointer needs no
  `Send`, and the one ordering rule is close's: mark the ring closed and
  wake waiters, then destroy the stream (SDL waits for a callback in
  progress and never fires one after), then let go of the ring clone the
  callback held.
- **A sample clock.** `opened_ns` stamps the open on the SDL tick timeline
  camera frames carry; `position` counts samples since. A module
  combining both (audio-visual models) aligns them without guessing.
- **Close wakes everyone.** `close_microphone` is the one close path (an
  app's `close()`, engine teardown through the plugin's `Drop`). Readers
  get `Closed` from `wait` and `Err` from `read_into`, so a module's worker
  ends with its engine and never holds up a reload. Buffered samples are
  dropped: a close is a stop.
- **JS `read()` is one reader.** The session's own reader, cursor 0 at the
  open. It keeps returning a `Float32Array`; a dropped count is logged,
  not surfaced, since a JS reader that polls at all never overruns a 10 s
  window.

### Speech onto the ring

The in-tree consumer, as QR scanning is for the camera tap. `speech.start`
still opens its own session; the recognizer worker takes a tap on it and
waits for `CHECK_INTERVAL` samples at a time instead of being fed per
frame. The worker's input becomes a `SampleSource` the tap and a channel
both implement, so the batch tests in `lattice/tests/speech.rs` still feed
clips through a channel. Worker events reach JS through a task spawned on
the JS executor draining a channel, which wakes without a frame; the
per-frame tick goes.

### A test source

`Context::install_test_microphone(sample_rate, samples)` makes a clip the
microphone, the still camera's twin: the device list is that one
microphone (id 0), every open at that rate is a session with no SDL stream
whose ring holds the clip from the open, a later install pushes the new
clip into every open test session, and an open at another rate fails
naming both rates (no resampling). No thread runs and nothing demands a
frame, so an app test settles. `@solidrt/test` exposes it as
`app.microphone({ sampleRate, samples })` over the `sol:test` verb
`microphone`. `packages/core/tests/microphone.test.tsx` runs a read, a
re-install and the close under `sol test`.

### A module over the ring

```rust
let mut tap = lattice::flux::gui::alloy_context(&ctx)
  .ok_or_else(|| throw(&ctx, "listen: no gui"))?
  .microphone_tap(session_id)
  .map_err(|e| throw(&ctx, &format!("listen: {e}")))?;
std::thread::spawn(move || {
  let mut chunk = Vec::new();
  loop {
    match tap.wait(CHUNK, WAIT_TIMEOUT) {
      MicWait::Samples(_) => {
        chunk.clear();
        let read = tap.read_into(&mut chunk).expect("open after a wait that saw samples");
        results.send(infer(&chunk, read.position));
      }
      MicWait::Timeout => {}
      MicWait::Closed => break,
    }
  }
});
```

Results reach JS as an async iterable (`attach_async_iterator`) or a
promise (`with_in_flight`), both of which wake JS without a frame. The
camera's results ride the frame tick because a streaming camera is
standing frame demand; a microphone is not.

## Rejected

- **An exclusive take**: `microphone_tap` takes the session's reader role,
  JS `read()` throws from then on, two consumers open two sessions. The
  smaller change, but a second model to explain beside the camera's, two
  conversions per device, and it leaves the unbounded buffer in place.
- **A thread-safe handle on the SDL stream** (the `PcmSinkHandle` shape):
  one reader per stream, since a get is a destructive drain, so it forces
  the exclusive take above.
- **A `sol-mic` thread draining the stream.** A thread for a wait SDL
  already offers.
- **A per-reader callback with the slice.** Runs the consumer inside SDL's
  audio thread; a slow model stalls capture.
- **A resampling test source.** The test picks the rate it opens at.
- **The module opens the microphone itself.** The app owns the device and
  its lifecycle, as with the camera.

## Done looks like

- A module in a custom runtime reads a session's samples on its own
  thread, every sample while it keeps up, a dropped count when it does
  not, and its worker ends when the session closes or the engine reloads.
- An open microphone session holds a bounded window; no poll obligation
  anywhere.
- Speech recognition delivers results on an app at rest, checked by hand
  on Linux with a real microphone and on the Android tablet.
- A microphone module runs under `sol test` over an installed clip.
- The "Native code" section of `docs/runtime/index.md` names the tap and
  the delivery route; `flux:microphone`'s types carry the id and the
  window rule.

## Order

1. The ring in alloy: types, producer, cursors, window, close. Unit tests
   in `alloy/src/tests/` for the ring on its own (push, read by cursor,
   two readers, overrun and dropped count, wait, timeout, close wakes
   waiters). The SDL put callback wrapper in `sdl_utils`.
2. The id on the session in `flux:microphone`, core and flux-types.
3. Speech onto the ring: `SampleSource`, the event task, the tick removed.
4. The test source, the verb, `app.microphone`, the core test.
5. Docs, then the device checks.

## Outcome

Built in the order above on 2026-10-11; the device checks landed on
2026-10-12. The ring on the Android tablet: a session opened through the
permission flow (okf/done/microphone-permission-state.md) delivered samples
at the requested rate to an app reading per frame. Speech on an app at
rest, with the Whisper tiny and Silero models (kept out of the tree, a copy
in the backup checkouts): on Linux with the USB microphone and on the
tablet, spoken sentences came back as final transcripts while the client
demanded no frames between results (the latest frame seconds old in
`/stats`). The speech feature had not been compiled since the move onto
the ring; it needed a one-line borrow fix in the worker's fill. The
batch test `lattice/tests/speech.rs` passes with the models; its process
segfaults at exit now and then when the four tests run in parallel, after
every result is printed (okf/tiny.md, Runtime).

## Open

- Nothing.

## Not in scope

- The output side: a JS streaming sink and a sink id a module pushes into
  (feedback item 7 of the same demo): okf/backlog/audio-pcm-sink-js.md.
- The Android recording permission flow: the open blocks the UI thread
  behind SDL's prompt and reports a denial as an error:
  okf/done/microphone-permission-state.md (done 2026-10-12).
- The in-tree speech lane leaving once an external module exists:
  okf/backlog/speech-out-of-tree.md.
