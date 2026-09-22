---
title: Move the texture video player off the frame loop
description: The texture player selects and uploads its frames inside the JS tick on the UI's clock, and predates the transport the plane player got - no reader, no anchor, no buffering, no seek, and a ~10% frame drop on audio-clocked streams. Browser style is one shared worker for both players over a presenter trait, decoded frames pushed to alloy with a due time on a clock both sides share, a raster-side latch against the frame's presentation deadline, standing demand held by alloy, close that never joins on either player, and the JS tick deleted. Presentation stays at the UI's cadence; a shared surface is a shared clock.
created: 2026-09-22
---

# Move the texture video player off the frame loop

The plane player ([[android-video-punch-through]], [[video-streaming]]) runs
the way a browser runs video: demux, decode, the clock, frame selection and
audio on their own threads, the UI never involved. The texture player does
not. Its frame selection and its texture upload run inside the JS tick
(`flux/src/alloy_plugins/video.rs::tick`, called from
`flux/src/alloy_plugins/frame.rs::advance` every frame), its master clock is
the UI's (the paced engine timeline for silent streams, the PCM sink's
position for streams with audio, both read on the JS thread), and its
standing frame demand is a flag the JS tick returns. The forge player
behind it (`forge/src/video/player.rs`) is the 2026-08 design: a decode
worker feeding bounded queues, and `advance(clock_us)` called per tick from
the JS thread to pick the due frame.

This item moves everything about the video off the JS thread and the frame
loop, the way the plane plan's section "The texture path, browser style"
sketched it, and in doing so gives the texture player what the plane player
already has: URL sources, buffering, seek, a real `error()`/`failed`, and
audio that corrects the clock instead of selecting frames. It also closes
the one blocking call both players still make on the JS thread, the join
in close.

## Status

Planned 2026-09-22. Built the same day, steps 1 to 5, on the host:
forge's worker over the `Presenter` (the plane's surface presenter, the
libvpx presenter, the MediaCodec buffer presenter), the injected clock,
the frame sink, close without a join on both players, alloy's clock and
YUV latch with the raster-side take against `present_at`, the three
counters, `present_at` through lattice and flux, the flux player over the
worker for both presents, the JS tick deleted, the docs. Host tests: forge
(the worker over a stub presenter, the texture player over libvpx and a
stub sink), alloy (the latch's take, evict, peek, playback waits). Android
arm64 cross-checked. Verified on the tablet the same day (Findings): the
plane's census against a baseline of the previous commit, seek, stall,
close during a stall, ten clip switches; the texture path's 60 s audio
clip with 0 skipped, the 50 and 25 fps cadences, standing demand alone.
NOT done: the TV (its census, lip sync by ear on avsync.webm through a
texture), and per-push demand, which was not built, so the A/B of section
4 is standing demand against the pump only.

## What this does not change

- **The picture is presented at the UI's cadence.** Video pixels that land
  in our composited frame are shown when that frame is shown; a shared
  surface is a shared clock ([[android-video-punch-through]], "The rule
  underneath it"). This is true in a browser too: the compositor samples
  the latest due frame at its own cadence. What moves off the frame loop is
  the decision which frame is due and the work of getting it there, so a
  slow JS frame, a stalled reactive flush or a heavy layout cannot delay
  decode, move the audio clock, or make frame selection double-step.
- **The TV's ceiling on this path.** Fullscreen 1080p through a texture
  still costs the 10.4 ms plane upload plus the 11-13.5 ms full-window draw
  per frame on the Philips TV ([[video-playback]], 2026-09-12 numbers);
  720p remains the ceiling there. The upload cost is
  [[texture-upload-leases]]; the draw cost is [[live-texture-content-damage]].
  Neither is this item, and both keep their consumer through it.
- **No video primitive.** `<texture src={video.texture()}>` stays the
  display path; the player exposes the same object shape as the plane
  player. `createVideo` in core already has the full shape (seek, seekable,
  buffering, error), so app code does not change.

## Why this is not a small change

1. **Frame selection and upload run inside the JS tick.**
   `video.rs::tick` reads the master clock, calls the forge player's
   `advance`, and pushes the due frame through `Context::update_yuv`, every
   frame, on the JS thread, inside `frame::advance`. A frame whose JS runs
   late selects late, and the standing demand that keeps the loop on the
   vsync grid is the `VideoTick::playing` flag that tick returns.
2. **The master clock is the UI's.** Silent streams clock on
   `timeline_now_ms` (the paced frame clock, refresh-counted); streams with
   audio clock on the sink position, which advances in device-buffer steps.
   Selecting per frame against that stepped position is the measured ~10%
   frame drop on both Android devices ([[video-playback]], "Open on this
   path"). The plane player fixed this by never selecting on audio: the
   sink position only nudges the anchor (`transport::AudioSync`).
3. **The forge texture player predates the transport.** `VideoPlayer`
   reads the demuxer inline on its worker (no `Reader` thread, so a source
   that stalls blocks decode), has no `Anchor`, no `AudioSync`, no
   `Buffering`, no seek, and publishes nothing; its consumer-side state
   (`staged`, the `STALLED_CALLS` tail rule) exists only because the caller
   drives it from a tick. The plane `Worker` in `plane.rs` has the loop we
   want, but its output stage is `releaseOutputBufferAtTime` on a
   MediaCodec surface, inlined into `feed` and `present`.
4. **The Android buffer-mode decoder blocks inside `decode`.**
   `MediaCodecDecoder::feed` waits on an input buffer in 10 ms steps up to
   `STALL_MS` (2 s) and `drain`/`flush` wait on output; the `VideoDecoder`
   trait is synchronous by shape (`decode(au) -> Vec<YuvFrame>`), which fits
   libvpx (one frame out per frame in) and not a hardware codec. A worker
   that called it inline would not drain pause, seek or close while the
   codec was busy.
5. **alloy's YUV upload is UI-thread bookkeeping.** `update_yuv` flips the
   front/back plane set in `yuv_groups`, notes content damage on the
   planes, sends `RasterCmd::UpdateYuv`, and rebinds the conversion target
   through `set_target_textures`, all on the UI thread, once per frame.
   Nothing on the raster side decides anything.
6. **The raster thread draws only on a `Frame` command from the UI
   thread.** There is no raster-only re-present: a new video frame reaches
   the screen only through a UI-thread frame build. So "the raster latches
   the newest due frame" needs the UI loop to build a frame for it, and
   something has to ask for that frame now that the JS tick will not.
7. **The two crates keep different clocks and share no edge.** forge's
   `transport::monotonic_ns` is CLOCK_MONOTONIC on unix (what Android's
   `releaseOutputBufferAtTime` takes) and a process-relative `Instant`
   elsewhere; alloy reads `std::time::Instant`. alloy does not depend on
   forge and will not. A due time pushed from a forge worker has to be
   comparable on alloy's raster thread by construction, not by two
   functions that happen to agree.
8. **Headless playback is deterministic today because the clock is the
   virtual frame timeline.** `srt render` steps frames on a virtual clock
   (D6 in [[frame-timing]]), and the tick's silent-stream clock follows it,
   so `examples/video/src/probe/render.tsx` captures the same frames every
   run. A worker on a wall clock would not.
9. **Close joins the worker on the JS thread.** `PlanePlayer::drop` closes
   the reader, sends Close and joins; the texture player will share the
   worker. The join is the open "close during a stall holds the JS thread
   ~1 s on the TV" item ([[video-streaming]], Status), and it exists for
   one ordering: the codec must have released the surface before the plane
   view is removed. The resources behind both players (the SDL audio
   stream, the plane view, the YUV texture) are owned by UI-thread
   registries the worker only borrows, which is why the UI has to wait.
10. **The JS surface is stubbed.** `build_player` in `video.rs` hard-codes
    `seekable: false`, `seek` as a no-op, `buffering` false, `error`
    undefined and a `failed` promise that never settles, and rejects a URL
    with `unsupported`.

## Design

### 1. One worker, three presenters (forge)

The plane `Worker` loop moves out of `plane.rs` into
`forge/src/video/worker.rs` unchanged in shape - poll the reader, feed
audio, drain commands, feed the decoder, judge buffering, present one
picture against the clock - and the things that differ between a plane, a
hardware buffer-mode decoder and a software decoder become one trait it
drives, defined in `worker.rs` as the worker's contract:

```rust
/// Where decoded pictures go, and how coded frames get there: a surface
/// MediaCodec renders into (the plane), a buffer-mode codec whose output is
/// copied out and handed to a FrameSink (Android textures), or a
/// synchronous decoder doing the same (libvpx). Owns the picture it has
/// dequeued until it is released or discarded, so the worker never holds a
/// codec buffer across a borrow.
pub trait Presenter {
  /// Take at most ONE coded frame from the reader when there is room, so
  /// commands drain between frames whatever decode costs. `Feed::Refused`
  /// with a packet waiting feeds the worker's wedge timer (INPUT_STALL).
  fn feed(&mut self, reader: &Reader) -> Result<Feed, StreamError>;
  /// Make the next decoded picture current, waiting up to `wait` for it.
  /// None when nothing is ready within the wait.
  fn next(&mut self, wait: Duration) -> Result<Option<Picture>, StreamError>;
  /// Release the current picture for the system time `due_ns` (on the
  /// worker's clock, see 2), snapping onto a vsync grid when it has one.
  fn release_at(&mut self, due_ns: i64) -> Result<(), String>;
  /// Drop the current picture unshown (a seek preroll frame, a late frame,
  /// an empty end-of-stream buffer).
  fn discard(&mut self) -> Result<(), String>;
  /// Forget everything queued or in flight (a seek).
  fn flush(&mut self) -> Result<(), String>;
  /// How far ahead of its due time a picture must be released: one
  /// compositor period, RELEASE_LEAD_NS when the period is unknown, and
  /// unbounded (i64::MAX) when the clock is stepped (see 8).
  fn lead_ns(&self) -> i64;
}

pub enum Feed { Took, Full, Refused { packet_waiting: bool }, End }
pub struct Picture { pub pts_us: i64, pub eos: bool, pub empty: bool }
```

- **`SurfacePresenter`** (`plane.rs`, Android) is the code in today's
  `Worker::feed` and `present`: `feed` is the input-buffer step with its
  "look before dequeue" rule, `next` is `dequeue_output_buffer(OUTPUT_WAIT)`
  holding the buffer inside the presenter, `release_at` snaps on the
  `VsyncGrid` and calls `release_output_buffer_at_time`, `discard` releases
  with `render = false`, `lead_ns` is `VsyncGrid::lead_ns`. It owns the
  codec AND the `VideoPlane` (see 7). No behaviour change on the plane; the
  TV census must read exactly as before.
- **`BufferPresenter`** (`mediacodec.rs`, Android) is the same codec in
  buffer mode with the same feed/next split: `next` dequeues one output,
  repacks it into a `YuvFrame` (the stride/slice-height/crop copy-out that
  exists today) and releases the codec buffer at once, holding the frame;
  `release_at` pushes the frame to the `FrameSink`; `discard` drops it. The
  codec input step is one function shared with the surface presenter (the
  dequeue-input, copy, queue sequence and the end-of-stream flag); today's
  blocking `feed`/`drain`/`flush` and their `STALL_MS` go, the worker's
  `INPUT_STALL` is the wedge rule for both modes.
- **`DecoderPresenter<D: VideoDecoder>`** (`texture.rs`, every platform;
  the libvpx `Vp9Decoder` is its only implementation and the `VideoDecoder`
  trait shrinks to what a synchronous decoder is) decodes the one coded
  frame `feed` takes into a small local queue (`PRESENTER_QUEUE`: two
  frames, since a 1080p frame is 3 MB and the sink queues ahead of it) and
  reports `Feed::Full` while it holds them; `next` pops the head;
  `release_at` pushes to the sink; `flush` clears the queue and resets the
  decoder.

Both texture presenters are opened over a `Box<dyn FrameSink>`, in
`transport.rs` next to `AudioSink`, engine-free:

```rust
/// Where a texture presenter's decoded frames go: the compositor that will
/// show them. Implemented by the platform (alloy's YUV latch, through the
/// flux adapter) and used from the worker thread. `push` never blocks
/// against a live compositor: the sink keeps a bounded queue ordered by
/// due time and evicts what can no longer be shown. Against a stepped
/// clock it blocks instead (see 8).
pub trait FrameSink: Send {
  fn push(&mut self, frame: YuvFrame, due_ns: i64);
  /// The pts of the frame the compositor last showed (see 3); what
  /// `currentTime()` reports for a texture player.
  fn shown_pts_us(&self) -> Option<i64>;
  /// Playback stopped or started: the sink holds or releases the
  /// compositor's standing demand (see 4).
  fn set_playing(&mut self, playing: bool);
  /// The compositor's refresh period, None when it has none (playback).
  fn period_ns(&self) -> Option<i64>;
  /// The stream ended: nothing more will be pushed (see 8).
  fn end(&mut self);
}
```

`VideoPlayer`, `player.rs` and its tail rule (`STALLED_CALLS`) go. The
worker's release policy is the transport's `classify` for every presenter:
`Wait` sleeps until the lead, `AtTime` releases, `Drop` discards,
`Reanchor` resets the anchor - the ExoPlayer shape the plane already runs.
A frame the worker drops (decode behind) and a frame the latch skips
(compositor behind) are both counted (7 and 3).

The worker takes its `Presenter`, `Clock` (see 2), optional `AudioTrack`
and `Reader` at open, so it is testable on the host with a stub presenter,
a stepped clock and a stub sink, which the plane worker is not today. Its
last act, whatever the exit, is `Shared::set_exited` (a flag plus the
existing `Notify`; `Shared::exited()` has the shape of `failed()`).

Module layout after: `worker.rs` (loop, `Presenter`), `transport.rs`
(clock, sinks, anchor, policy), `plane.rs` (surface presenter +
`PlanePlayer`, Android), `mediacodec.rs` (codec input step, buffer
presenter, Android), `texture.rs` (`DecoderPresenter` + `TexturePlayer`),
`vpx/` unchanged, `player.rs` gone.

### 2. The clock is injected, and it is alloy's

The worker's reads of `monotonic_ns()` and its waits (`wait_for_release`,
`start_audio`'s poll) go through a `transport::Clock` the player is opened
with:

```rust
pub struct Clock {
  /// The reading frames are scheduled on, in nanoseconds.
  pub now_ns: Box<dyn Fn() -> i64 + Send>,
  /// The clock advances only when its consumer steps it (headless playback):
  /// the worker never sleeps against it and never lets audio correct it.
  pub stepped: bool,
}
```

- The plane player's clock is `monotonic_ns` (Android's `System.nanoTime`,
  which `releaseOutputBufferAtTime` requires), as today.
- The texture player's clock is alloy's: `alloy::clock::now_ns()`, a
  process-relative monotonic reading (`Instant` against a process epoch,
  with `alloy::clock::ns(Instant)` for converting the instants alloy's
  loop already holds), handed to forge by the flux plugin as the closure.
  The raster thread's latch and the worker's due times are then the same
  clock by construction, with no shared definition to keep in step and no
  new crate edge. In headless playback the closure reads the virtual frame
  time (see 8).

`Anchor`, `AudioSync`, `VsyncGrid` and `classify` are already pure over
`now_ns` values and do not change.

### 3. The latch in alloy, against the frame's deadline

alloy grows one mode on the YUV texture: a latch. `Context::yuv_frame_sink
(id) -> YuvFrameSink` returns a `Send` handle (the `PcmSinkHandle`
precedent); flux wraps it in a `FrameSinkAdapter` implementing forge's
`FrameSink`, the way `SinkAdapter` wraps the PCM handle (alloy never
implements a forge trait). Behind it, shared with the raster side through
an `Arc<Mutex<..>>` registered with `RasterCmd::AttachYuvLatch { id,
latch }`:

```rust
/// Frames waiting to be shown, ordered by due time, at most
/// LATCH_QUEUE_FRAMES (4: a lead of one period plus the content interval
/// never queues more at any rate up to the refresh rate). Against a live
/// compositor a push past the cap evicts the oldest; the worker never
/// blocks on the compositor. Closed at destroy: a push is then a no-op.
struct YuvLatch {
  queue: VecDeque<LatchedFrame>,   // due_ns, pts_us, packed frame bytes
  shown_pts_us: Option<i64>,
  playing: bool,
  ended: bool,
  closed: bool,
}
```

At `RasterCmd::Frame`, inside `frame()` for the frame that draws (the
batch rule keeps only the newest queued frame, and its deadline with it),
before `flush_dirty`, the raster thread takes for every latch the newest
frame due at or before the frame's deadline (below) plus `LATCH_LOOKAHEAD`
(half a refresh period: the frame belongs to the present whose vsync is
closest to its due time, the same nearest-vsync rule the plane's snap
implements and the half-period lookahead the JS tick measured 2.8% ->
0.07% flips with), uploads it into the back plane set, flips the set and
rebinds the conversion program's samplers on the raster side, and marks
the output dirty so the ordinary flush re-renders it and everything
sampling it. Frames older than the taken one are skipped and counted
(`videoSkipped` in `RasterStats`, next to `videoLatched`; both reach
`/stats` and the stats overlay). The taken pts is written to
`shown_pts_us`, which is what `currentTime()` reports: the frame on
screen, not the frame last handed over. That is the read the pacing
probe's `cadence` command samples, so duplicate or skipped content shows
in it directly (the SF census and the engine fps are blind to both).

**The deadline is the frame's presentation estimate, not a wall read.**
The raster thread reads no clock at all: it compares due times against
`present_at`, the instant the frame being drawn is expected to reach the
screen, carried in the `Frame` command. alloy's main loop knows it: the
frame signal's reference instant (a vsync under VsyncLocked, the present
return under SwapPaced, the tick under idle) plus the cadence hold's
periods (`FrameRelease::hold`, D9 in [[frame-timing]]: a held frame
presents at its slot end). `AlloyEvent::FrameRendered` and `Tick` gain a
`present_at: Instant`; lattice carries it in `RenderFrame` next to
`period_ms`; flux's `frame::draw` hands it to `Context::submit` and
`submit_clean` with the display list. Selection is then a pure function of
the signal grid - the grid the app timeline itself advances on (D1), so
video content and app animation step on one clock - and immune to the
execution jitter that made a wall read inside the tick hold and
double-step frames. When presentation feedback arrives
([[presentation-feedback]]) the estimate becomes a measurement without
changing the rule.

The latch is the only way pixels reach a YUV texture: `update_yuv` and
`RasterCmd::UpdateYuv` go, every YUV texture has a latch from creation
(`create_yuv_texture` attaches it; `yuv_frame_sink` hands out handles to
it), and a producer with no clock of its own pushes with a due time of
zero, which latches at the next frame. One path, no mode on the texture,
nothing to keep in step with the mirror beyond the note below.
`set_target_textures` is untouched for every other target.

**The UI-side mirror keeps set 0 bound.** The context's sampler-graph
mirror exists for the content closure and the cycle check; both plane
sets have identical edges into the output, so the raster owning the real
binding in latch mode diverges from the mirror harmlessly. `YuvGroup`
says so in its comment.

### 4. Demand: the push asks for the frame, alloy holds the standing demand

The raster draws only on a UI-thread `Frame`, so a pushed frame must get
one built. `YuvFrameSink::push` latches the platform's frame request
(`PlatformContext::frame_request_handle`, an `Arc<AtomicBool>` already
callable from any thread) and wakes the main loop through the same custom
SDL event `Context::submit`'s wake closure pushes (`FrameReady` in
`app.rs`), shared with the sink as an `Arc<dyn Fn + Send + Sync>`. The
next frame signal builds the frame; under VsyncLocked that is the next
vsync, when idle it is at most one refresh period away through the idle
tick. The desktop's idle cadence has a known defect
([[idle-onframe-tick-rate]]: 2-3 Hz where a refresh period is promised);
it can only touch the first frame after play or after buffering, and
first-frame latency is measured under Verification so it is caught if it
does.

While a texture player plays, the demand is standing, and alloy holds it:
`set_playing(true)` on the sink marks the latch playing, `Context::
streaming_textures()` reports whether any latch is, and flux's draw gate
(`frame::draw`, the `standing` line) requests the next frame on it exactly
as it does for running transitions. The JS tick's `VideoTick::playing`
goes.

Standing demand rather than one request per push is the default because
it is what was measured: demanding only on upload halved the presents on
the TV and looked worse, because presents then landed wherever the loop's
phase had walked to (27 at one period, 76 at two, 21 at three, against
124 of 125 on the grid with standing demand; the comment in
`frame.rs::advance`, deleted with the tick and recorded here). That was
measured under the JS tick's clock. Under the latch the request is served
on the signal grid and selection is deadline-based, so per-push demand
may hold the grid now, and it is worth half the presents on the TV. The
choice is re-measured as part of this item, not left to a later one: the
pacing probe's pump lever (a no-op `onFrame` is standing demand) is the
A/B, the census and `videoSkipped` are the readings, both go into
Findings, and per-push ships if it holds the grid. Frames with deadlines
through the whole loop ([[frame-driver-pacing-contract]], stage 2) remain
the general answer; `present_at` is its first half.

### 5. Content damage: the gate peeks, the raster latches

Damage for a texture node showing fresh pixels, and the re-raster of a
cached boundary (a snapshot, a repaint boundary) that baked those pixels,
both come from `note_content` on the UI thread before the frame builds
(`take_content_changes` -> `RenderTree::texture_content_changed`). The
raster's latch runs after the build, so the UI thread has to know in
advance. It does: the draw gate peeks every playing latch with the same
deadline the raster will use (`Context::yuv_frame_due(id, present_at)`:
head due time against deadline plus lookahead, a mutex read) and notes
content on the OUTPUT id, never on the planes. The raster then takes by
the same rule with the same deadline, so the two agree by construction.
The one case they cannot is a frame pushed for this slot after the gate
peeked: a worker that missed its lead. The raster shows it anyway (a live
`<texture>` samples the new pixels without damage), a cached boundary
holding it lags one frame, and the miss is counted (`videoLateLatches`,
the third counter) so it is a number, not a suspicion.

### 6. Audio moves onto the worker

The worker owns the `AudioTrack` over the flux `SinkAdapter` (a
`PcmSinkHandle`, already `Send`) exactly as the plane player does: the
track is fed to `AUDIO_LOOKAHEAD_US` between releases, the first frame
after play, a seek or buffering waits for the sink to consume and anchors
on its content time (`start_audio`), and `AudioSync` moves the anchor once
when the smoothed lead crosses `AUDIO_SYNC_THRESHOLD_US`. Audio never
selects frames, which is what removes the ~10% drop. `PCM_LOOKAHEAD_US`
and the sink feeding in `tick` go; `set_pcm_sink_paused` from `play_impl`
goes (the track pauses its own sink).

`AUDIO_OUTPUT_LATENCY_US` is one constant for both players. The texture
picture reaches the screen one to two vsyncs after its latch, the plane's
about two after its release; the difference is inside the sink position's
granularity. Verified with `examples/video/assets/avsync.webm` on the
desktop and the TV (the constant was set on the TV's speakers through the
plane); the `audioDelay` option in okf/tiny.md applies to both players
when it lands.

### 7. Close returns at once, on both players

`close` sends `Command::Close` and returns; nothing joins, on either
player, at engine teardown included. Everything the worker uses is owned
by whoever holds it last, so no UI-thread registry has to wait:

- **The SDL audio stream** becomes `Arc`-owned (`PcmStream`, whose `Drop`
  destroys it; SDL's stream calls are thread-safe): the registry entry and
  every `PcmSinkHandle` hold a clone, `destroy_pcm_sink` removes the
  registry entry, and the last holder destroys the stream on whatever
  thread it drops on. The handle's contract ("must be dropped before
  destroy") goes with the reason for it.
- **The plane view** is owned by the surface presenter, after the codec in
  field order, so dropping the presenter on the worker releases the
  surface first and then removes the view. `VideoPlane` is already usable
  from any thread (`create`/`Drop` go through `SDL_GetAndroidJNIEnv`,
  which attaches the calling thread; Java's `destroyVideoPlane` posts to
  its UI thread), and its fields (a `Global` ref, a `NativeWindow`) are
  `Send`; the type says so. The one ordering close used to guarantee, "a
  clip switch opens the new plane after the old view's removal was
  posted", becomes a rule in the flux plugin: a plane open whose predecessor
  is still closing awaits that player's `exited()` before it creates its
  plane. `exited` is set after the presenter dropped, so the removal is
  posted before the creation is, and Java's one-plane check holds.
- **The YUV texture's borrow** (`release_borrowed`) is released at close on
  the UI thread and the latch is marked closed, so a worker still pushing
  hits a no-op until it reads Close. The texture reclaims at the next sweep
  as any borrowed texture does.
- **The reader** is closed first (`ReaderHandle::close`), as today, so a
  blocked read returns at once.

`PlanePlayer::drop` and `Inner::drop` stop joining; the plugin's `failed`
watcher gains nothing, since resources no longer wait on the exit. The
TV's "close during a stall holds the JS thread" item closes for both
players, and the close cost becomes a measurement in Done.

### 8. Headless playback stays deterministic

`srt render` (playback mode) steps frames on a virtual clock. The texture
player's `Clock` there reads the virtual frame time (the `playback_frame`
counter lattice already publishes before each clock read, times the
capture period) and is `stepped`, which changes three things:

- The worker never sleeps against the clock: `lead_ns` is unbounded, every
  decoded frame is pushed as soon as it is decoded, and audio never
  corrects the anchor (the sink is a real-time device that cannot clock a
  virtual timeline; the track is still fed, so a capture that records
  sound has it).
- `push` BLOCKS when the latch is full instead of evicting: eviction would
  drop frames the capture has not taken yet, and blocking is the honest
  backpressure when the consumer steps the clock. Deadlock is impossible
  by the sizes: a full latch of four frames always holds one due after the
  deadline plus lookahead, or the take pops them all and makes room.
- The raster's take WAITS (a condvar on the latch) until the queue holds a
  frame due after the deadline plus lookahead, or the stream has ended
  (`FrameSink::end`), so a captured frame always shows the frame that is
  due and never the frame that happened to have arrived. The capture's
  `present_at` is the frame's virtual time itself. In interactive mode the
  take never waits.

`render.tsx` captures the same frames every run, and that is a test
(Verification). It compares frames, not exit codes: a headless run that
throws exits 0 today ([[render-as-a-verification-gate]]), so the check is
a diff of the captured frames against a second run and against a checked
expectation of the frame at a known time.

### 9. The JS surface (flux, core, types)

- `open(source)` for a texture player opens asynchronously on the reader
  thread like a plane open: the header comes off the JS thread, `signal`
  aborts, `OPEN_TIMEOUT_MS` applies, and a URL is accepted (the
  `unsupported` rejection and its message go). `build_player` creates the
  YUV texture, its frame sink, the PCM sink handle, the presenter for the
  platform and the worker, and builds the player object with the same
  builder the plane uses (`build_plane_player`'s body becomes
  `build_transport_object(shared, controls, extras)`; the two objects
  differ by `texture` and the plane's `finished || lost` read). `seekable`,
  `seek`, `buffering`, `error`, `failed` and `hasAudio` are then real on
  both.
- `currentTime()` reads the latch's `shown_pts_us` for a texture player
  (the frame on screen), the worker's position for a plane (the frame
  released), and the types say so.
- Docs, all of them: `flux-types/gui/video.d.ts` (`VideoPlayer`'s "on the
  UI's clock" becomes "decoded off the frame loop, shown at the UI's
  cadence"; `VideoSource`'s "a URL plays on a plane only" goes; `open`'s
  rejection list drops the texture-URL case; `VideoOpenOptions.signal`'s
  aside about texture players opening at once goes);
  `packages/core/src/video.ts`'s header; `forge/src/video/mod.rs`'s
  header (the two-player description); `transport.rs`'s doc (the "when its
  frame selection moves off the frame loop" clause becomes past tense);
  `alloy/src/yuv.rs`'s doc (the latch mode); `flux/src/alloy_plugins/
  video.rs`'s module doc; `packages/cli/agents/debugging.md` (the three
  `/stats` counters and what a fluency verdict on a texture reads from);
  `okf/backlog/video-playback.md` (the layering section and "Open on this
  path"); `examples/video/src/probe/pacing.tsx`'s header (what `cadence`
  now samples).
- `examples/video/src/index.tsx` stays as it is (it is the smallest
  texture app); `projects/video-streaming`'s texture fallback drops its
  "unsupported until the rework" copy and plays the URL.

### 10. What is deleted

`forge/src/video/player.rs`; the blocking `feed`/`drain`/`flush` and
`STALL_MS` in `mediacodec.rs`; alloy's `update_yuv`, `RasterCmd::UpdateYuv`
and the `front` field of `YuvGroup` (the raster owns the flip);
`flux/src/alloy_plugins/video.rs`'s `tick`,
`VideoTick`, `clock_now_us`, `PCM_LOOKAHEAD_US`, the `base_us`/`origin_us`
clock in `PlayerEntry`, the `timeline_now_ms` read and the module doc's
three-properties paragraph; the video branch of
`flux/src/alloy_plugins/frame.rs::advance` with its comment (the
measurement it cites is in section 4 here); the `period_ms` parameter of
`frame::advance` and lattice's argument for it (video was its only
consumer; `judge_period_ms` for `RenderFrame` stays); the joins in
`PlanePlayer::drop` and `Inner::drop`; `PcmSinkHandle`'s ordering
contract. `plane.rs` shrinks to the surface presenter and the open.

## Not in this item

- **Rate, step and settable currentTime** on either player: the worker's
  anchor gets a slope, step is one release while paused. Designed once for
  both after this lands ([[android-video-punch-through]], follow-ups); the
  anchor and the presenter contract here leave room for both (a slope is
  one field on `Anchor`, a step is one `release_at(now)` while paused).
- **`present: "plane"` off Android** presenting the texture player
  fullscreen behind the UI: additive on this item, listed in the plane
  plan's follow-ups.
- **Mid-stream resolution change** on the texture path: a decoded frame
  whose size differs from the texture's ends the stream with `unsupported`,
  matching the plane; an id-stable YUV resize is the later addition
  ([[video-streaming]], "Not in this item").
- **The camera on the latch**: the latch is video-agnostic (any producer
  with due times; a camera frame is due now), and the camera pump could
  push through it and leave `frame::advance` too. Not required for video
  and not touched here; after this item the camera and the gpu settle are
  the only device ticks left in `advance`.
- **The Windows libvpx build** (`forge/build.rs` panics with the `video`
  feature on Windows) and the per-platform hardware decoders
  ([[video-playback]], staging 3). The `DecoderPresenter` is where a
  VideoToolbox or VA-API decoder would plug in, or a second buffer-mode
  presenter where the API is asynchronous like MediaCodec's.
- **Lifting the dist gate** on the `video` feature: [[video-playback]].
- **Presentation feedback** replacing the `present_at` estimate:
  [[presentation-feedback]].

## Order of work

Every step leaves the tree building and the tests green; the plane is
verified on a device after step 1 before anything else touches it.

1. **forge: the shared worker and the plane on it.** `Clock`, `FrameSink`
   in `transport.rs`; `Presenter`, `Feed`, `Picture` and `worker.rs`
   extracted from `plane.rs`; the surface presenter owning codec and
   plane; the shared codec input step; `Shared::exited`; `PlanePlayer`
   without the join. Host tests with a stub presenter and a stepped clock:
   the release policy end to end (wait, at time, drop, re-anchor),
   first-frame anchoring with and without a sink, seek (`skip_until`, one
   release while paused), buffering start and end over a scripted reader,
   close during a wait returning at once and the presenter dropping on the
   worker, end of stream, the `exited` flag on every exit path (close,
   end, failure, lost surface). Then the TV and the tablet: the plane's
   VIDEO census reads as before (every interval two periods for 25 fps on
   50 Hz, strict 2/3 on 60 Hz), sync on `avsync.webm`, seek, stall, close
   during a stall with the JS thread's cost read, and a clip switch (close
   then open) ten times without a `no-plane` rejection.
2. **forge: the texture presenters and player.** `texture.rs` with
   `DecoderPresenter` over libvpx, `BufferPresenter` in `mediacodec.rs`,
   `player.rs` deleted, the `armv7-linux-androideabi` cross-check for the
   Android side. Host tests with the libvpx fixture: frames reach a stub
   sink with due times spaced at the content interval, a seek pushes the
   target frame once while paused, `end` follows the last frame, a URL
   source through the reader plays, a stepped clock pushes every frame
   without a wait.
3. **alloy: the latch.** `alloy::clock`, `YuvLatch`, `yuv_frame_sink`,
   `AttachYuvLatch` sent by `create_yuv_texture`, `update_yuv` and
   `RasterCmd::UpdateYuv` deleted, the raster-side take/upload/flip/rebind
   at `Frame`,
   `yuv_frame_due` for the gate, `streaming_textures`, the three counters,
   the playback wait and the blocking push, `PcmStream` ownership,
   `present_at` on `FrameRendered`/`Tick` and on `Context::submit`/
   `submit_clean`. Unit tests on the take rule (newest due wins, stale
   skipped and counted, nothing due, the lookahead at the boundary, closed
   latch drops pushes, the playback wait ending on a later frame and on
   `end`, the blocking push releasing on a take), and
   `alloy/examples/yuv_texture.rs` moved onto the sink: its uploads become
   pushes with a due time of zero, a pushed frame with a deadline latches
   and converts, a frame due later does not, and a stream's pcm handle
   outlives its registry entry.
4. **lattice and flux: the frame's deadline through the loop.**
   `RenderFrame.present_at`, `frame::draw` passing it to submit, the gate's
   peek and note, the standing demand read, the `period_ms` parameter of
   `advance` removed.
5. **flux: the player.** Async texture open on the reader, the shared
   object builder, the worker wired to alloy's clock, sink and frame sink,
   the plane open awaiting a closing predecessor, the deletions of section
   10, the docs of section 9.
6. **Verification** below, desktop first, then the TV and the tablet,
   including the demand A/B of section 4 with its numbers in Findings.

Step 4 can run beside 2 and 3; 5 needs all of them.

## Done looks like

- No video code runs in the JS tick: `frame::advance` has no video branch,
  and a texture player plays with the JS thread held for 200 ms by a debug
  command without the audio stopping, the position pausing, or more frames
  skipped than the slots the UI missed.
- Audio-clocked streams skip nothing: `videoSkipped` stays 0 over a 60 s
  run of `clip720.webm` on the TV and the tablet, where the tick dropped
  ~10% (45 of 452 and 44 of 420), and `videoLateLatches` stays 0 on the
  desktop.
- The pacing probe's `cadence` read shows `currentTime()` stepping an even
  20 ms for clip C (50 fps) and 40 ms for clip D (25 fps) on the TV, with
  the presents on the grid as in [[video-playback]]'s 2026-09-12 numbers,
  and the demand A/B recorded.
- The plane's census is unchanged after step 1.
- `close` returns in under a millisecond on both players during a stall,
  on the TV, measured through the JS timing the debug command reports; a
  clip switch on the plane never rejects with `no-plane`.
- The first frame after `play` and after a buffering episode shows within
  two refresh periods of the push on the desktop (the idle-cadence check).
- The texture player seeks, buffers and plays a URL: `projects/
  video-streaming`'s texture fallback plays the served clip on the desktop
  with the same stall and seek checks the plane passed on the tablet.
- `srt render` of `examples/video/src/probe/render.tsx` produces identical
  frames on two runs, and the frame at a known time matches its
  expectation.
- Lip sync on `avsync.webm` on the desktop by eye and ear, and on the TV
  through a texture at 720p with no anchor move logged after the start.

## Verification

Desktop: `examples/video` under `srt run` for the smoke test; the pacing
probe (`examples/video/src/probe/pacing.tsx`, its `cadence`, `clip` and
`pump` debug commands) for the content steps and the demand A/B;
`projects/video-streaming` with its `server/faults.ts` for URL, stall and
seek on a texture; `avsync.webm` for sync; `srt render` on
`probe/render.tsx` for determinism, diffed against a second run and
against the expected frame. Two debug commands are added to the pacing
probe: one that sleeps the JS thread for the stall check, one that reports
the wall cost of `close`.

Devices: the build and install loop, the SurfaceFlinger census for the
plane, `logcat` for the codec name and `[forge::video]` anchor lines, and
the measurement traps (MCP round trips of 5-8 s to the TV, bracketed
position reads) are in [[android-video-punch-through]] and
[[video-playback]]. `/stats` carries `videoLatched`, `videoSkipped` and
`videoLateLatches`, so a fluency verdict on the texture path is a counter
read over a run, not a census.

## Findings

- **The presenter cannot own the codec it holds a buffer of.** ndk's
  `OutputBuffer` borrows its `MediaCodec` and keeps its index private, so a
  presenter that holds a dequeued picture across worker calls cannot own
  the codec (a self-borrow), and the safe API offers no index to hold
  instead. The split is `PresenterHost` (owns the codec, the surface, the
  sink; built by the factory and dropped on the worker after the
  presenter) and `Presenter` (borrowed from it for the run). The texture
  presenters own everything and are their own host through a blanket
  `impl Presenter for &mut P`.
- **The frame sink needs `flush`.** A seek re-anchors, so its frames are
  due before the frames still queued in the latch; without a flush the
  latch would show the target frame and then the stale ones after it (the
  take rule reads due order). `FrameSink::flush` and `YuvFrameSink::flush`
  are the presenter's flush reaching the latch, next to the codec flush.
- **`snap` belongs before the wait.** The plane snaps the release time
  onto the vsync grid; the worker must wait against the SNAPPED time (up
  to 0.8 period earlier than the due time), or a due time just past a
  vsync hands the buffer over after the compositor's deadline for it and
  slips a period. `Presenter::snap` gives the worker the time it releases
  for; `release_at` takes that time.
- **`set_playing` comes from the worker, and from the caller too.** The
  worker tells the sink when releases start and stop (playing, not
  buffering, not failed, not ended), before it parks. The flux plugin also
  sets it at `play()` synchronously: a stepped capture's take waits only
  while the latch is playing, and the worker's first pass is a thread hop
  away, so without the synchronous set the first captured frame after play
  would race the worker.
- **The stepped take pops as it waits.** A full latch whose frames are all
  due at or before the deadline would deadlock the blocking push; the take
  pops every due frame first (keeping the newest as its candidate), which
  makes room, and only then waits for a frame due after the deadline or
  the end.
- **An instant decoder trips the buffering rule.** A stub presenter that
  decodes in zero time outruns the reader thread over a local file and
  drives the lead under the low water mid-clip, so the worker re-anchors
  once; the stub charges a millisecond per picture, as a real decoder
  would.
- **The 90 ms clock jump is absorbed.** With a 50 ms lead and 40 ms
  frames, a clock jump under lead plus one interval plus the drop
  threshold drops nothing: the frame in hand goes out at its time and the
  next is within the drop window. The policy test jumps 200 ms.
- **The texture lead is two periods, not one.** The plane's compositor
  takes a hand-over up to the vsync, so one period suffices there. The
  texture's "compositor" is the UI thread's frame build, which starts one
  period before the present and whose content check (the gate's peek)
  needs the frame at its start, for due times up to half a period after
  the present. With a one-period lead half of 50 fps content on the 60 Hz
  desktop was latched behind the peek (`videoLateLatches` 523 of 1119);
  with two periods, 1 (the first frame after play, which has no lead by
  nature). `TEXTURE_LEAD_PERIODS` in texture.rs.
- **The stepped take must pop before it waits.** The first headless render
  hung after one frame: a full latch of due frames, the take waiting for
  a frame due after the deadline, the producer's push waiting for room.
  The take pops every due frame (keeping the newest) before each wait.
- **Frames without a signal take the clock's instant.** The mount frame
  and the direct render path have no frame signal and used a real
  `Instant::now()`, which in playback compared a real elapsed time against
  virtual due times. `alloy::clock::now()` is virtual in playback.
- **Desktop measurements 2026-09-22** (60 Hz laptop, the pacing probe,
  pump on unless said, `/stats` counters over the run):
  - clip C (50 fps): `vidStepsMs` {20: 600, 0: 120} over 720 vsyncs, no 40s;
    videoSkipped 0, videoLateLatches 1, missedPresents 0.
  - pump off, 10 s: 501 frames latched on the player's own standing
    demand, no late latch added.
  - clip D (25 fps): {40: 300, 0: 421}, no 80s; skipped 0, late +1.
  - clip720 (25 fps WITH audio, the case the tick dropped ~10% of): {40:
    372, 0: 527}, no 80s; skipped 0. Audio now corrects the anchor
    instead of selecting frames.
  - `srt render` of `probe/render.tsx` (5 fps, 2 s): two runs identical
    frame for frame; frame 9 (virtual 2.0 s) shows the clip's burned-in
    00:00:02.000 / frame 50.
  - A 200 ms JS stall (the probe's `stall` command) while clip C plays:
    `currentTime()` advanced 13.92 -> 15.18 s over the 1.26 s the read
    took, `playing` stayed true, videoSkipped +10 for missedPresents +10
    (the ten 50 fps frames due during the stall's ten missed presents).
  - `close` (the probe's `openSecond` + `closeCost`): 0.045 ms for the 720p
    clip with audio at 1.88 s in, 0.042 ms for clip C.
  - The demand A/B of section 4 could not be read on the desktop beyond
    the counts above (the cadence histogram is recorded by the pump
    itself); see the tablet below.
- **Tablet measurements 2026-09-22** (SM-T500, Android 12, 60 Hz, dev
  client with its badge, the pacing probe over the LAN dev server):
  - clip720 (25 fps WITH audio, the case the tick dropped 44 of 420 on),
    pump on, 60 s: `vidStepsMs` {40: 1544, 0: 2225} over 3771 vsyncs, no
    80s; videoSkipped +0, videoLateLatches 0. The drop is gone.
  - Pump off, 20 s: 502 frames latched (every one), and the UI layer's
    SurfaceFlinger census reads 126 presents at one-period intervals: the
    player's standing demand presents every vsync, exactly as the pump
    does. Per-push demand was not built, so the A/B of section 4 is
    standing demand against the pump only; the two are the same picture.
  - clip C (50 fps): {20: 902, 0: 191} over 1093 vsyncs, no 40s. clip D
    (25 fps): {40: 452, 0: 643}, no 80s. Skipped 0, late 0 on both.
  - The plane (the streaming project over the paced fault server, the
    avsync clip): a seek lands and plays after the paced source catches up
    (buffering meanwhile, as it must on a source that serves at playback
    pace); a 6 s stall buffers and resumes; close during a stall leaves no
    slow frame beyond the page switch's own 19 ms (it held the JS thread
    0.36 s before); ten clip switches (close then open) all played, none
    rejected with no-plane.
  - The plane's census with only the dev badge on the UI layer read
    below the ladder in [[android-video-punch-through]] (94% for that
    row), so the previous commit's client was built in a worktree and
    censused back to back with the new one, same server, same clip,
    overlay off, three 5 s windows each: previous 75%, 86%, 90% on the
    25-on-60 pattern; new 82%, 85%, 81%. The same distribution: the plane
    is unchanged, and this tablet's day reads lower than the ladder's for
    both (the launch's own UI presents are inside the first window).

## Related

[[video-playback]] (the texture path's record; its "Open on this path"
list is superseded by this item for seek, audio-clocked drops and the
transport, and keeps the TV ceiling and the Windows build),
[[android-video-punch-through]] (the worker and transport this builds on,
and the section that sketched this item), [[video-streaming]] (the reader
and byte source the texture player inherits; its open close-stall item
closes here), [[frame-timing]] (D1, D6, D9),
[[frame-driver-pacing-contract]] (deadlines through the loop),
[[live-texture-content-damage]], [[texture-upload-leases]],
[[presentation-feedback]], [[idle-onframe-tick-rate]],
[[render-as-a-verification-gate]].
