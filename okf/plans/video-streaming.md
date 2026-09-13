---
title: Stream video from an HTTP source on the plane
description: Client-initiated streaming - open(url) plays a WebM served over HTTP on the Android plane with buffering, seek by Range and a cancel-safe close, built on a generic forge byte source and a reader thread that the texture player's browser-style rework reuses; live streams are deferred as an explicit latency option.
created: 2026-09-13
---

# Stream video from an HTTP source on the plane

Both players open local paths only (`WebmDemuxer::open(path)` reads through
`fs::open_seekable`). An app that plays a clip from a server has to download
the whole file first and open the copy, which is what
`projects/video-streaming` did before this item. Streaming means the player reads the
network itself, starts after a short buffer and never needs the whole file.

## Status

- Built (steps 1-3, 2026-09-13): Design 1-7 in full - byte source, source
  rule, demuxer, reader thread, buffering, plane player, JS surface - with
  forge tests. Departures are under Findings.
- Test app (step 4): done. `projects/video-streaming` opens the clip by URL
  with no copy, shows buffering, and falls back to a texture only on
  `no-plane`. Debug commands (open, close, seek, toggle, state) drive it,
  and `server/faults.ts` serves a paced, stallable avsync.webm and a URL
  that never answers.
- Tablet (step 4), verified 2026-09-13:
  - The served clip plays from its URL start to finish: the VIDEO census is
    strict 2/3 alternation (23.976 fps on 60 Hz) and memory is flat.
  - An ffmpeg `-listen 1` HTTP feed plays as an unseekable stream: first
    frame 0.65 s after open, no buffering, clean end when ffmpeg stops.
  - Seeks land on the first frame at or after the target: inside and past
    the read-ahead, backward, paused or playing (after the fix under
    Findings).
  - A 5 s server stall gives one buffering episode; a 12 s one gives one
    episode and a single reconnect at the stalled byte with If-Range.
    Neither drops or re-anchors a frame.
  - Close during a stall or a pending open returns at once (tearing the
    plane down holds the JS thread about 0.3 s), a reload during a stall
    brings the app back in 1.4 s, a hanging open fails with `network` after
    15 s, and seek on an unseekable stream does nothing.
  - Every start of playback is in step with the sound: the first start,
    play after a pause, a paused seek then play, a seek while playing and
    the resumes after both stalls log no audio-sync anchor move (after the
    change under Findings).
- Desktop, verified 2026-09-13: the texture player rejects a URL with
  `unsupported`.
- TV (step 4), verified 2026-09-13 with the same code as the tablet:
  - avsync.webm from the fault server: the VIDEO census is every interval
    two periods on the 50 Hz panel (one run had a single 3-then-1 pair).
  - By eye and ear the beep is on the flash straight through, right after a
    5 s server stall (one buffering episode) and right after a 3 s pause;
    the log shows no audio-sync anchor move, re-anchor or drop for those.
  - Seeks land on the first frame at or after the target, paused or
    playing; seek on an unseekable stream does nothing.
  - A 12 s stall gives one episode and a single reconnect with If-Range.
  - Close during a pending open returns at once and the next open plays, a
    hanging open fails with `network` after 15 s, and a reload during a
    stall brings the app back in 1.8 s.
- TV, open: a close during a stall holds the JS thread about 1 s (about
  0.4 s during playback), and two stream starts each logged one anchor move
  just past the 40 ms threshold (Findings).
- Not yet run on either device: memory under the byte cap at a high
  bitrate.

## Two modes

1. **Client-initiated streaming (this item).** The app opens a URL when the
   user decides; the stream starts at its beginning. Every frame plays and
   nothing is skipped to catch up. The source is a file served over HTTP with
   Range support (seekable, known length); an unseekable source (a chunked
   response with no length) plays too, without seek.
2. **Live streaming (deferred, will be built).** Hooking into a running
   stream: joining mid-stream at a keyframe, holding a latency target, catching
   up by dropping. What it adds, and what this item already holds for it, is
   the last section.

The mode is explicit, never inferred. Measured 2026-09-13 with ffmpeg n8.1.2:
ffmpeg writing WebM into a pipe (a mode-1 feed) produces an unknown-size
Segment with a false Duration (the input's 242.47 s for 20 s of content),
`-live 1` output is the same stream without a Duration, and both go out
chunked with no length. Neither the container nor HTTP separates the modes,
so inferring "live" would skip content on a mode-1 feed.

Decisions from [[android-video-punch-through]] kept: one WebM demuxer, no
AMediaExtractor, a shared transport, the same player object, no video
primitive, `seek` a no-op on a source that cannot seek.

## Why this is not a small change

1. **Blocking reads sit inside the player loop.** The plane worker reads the
   demuxer inline and drains commands only between reads, and
   `PlanePlayer::drop` joins that worker on the JS thread (close_plane,
   `Inner::drop`, core's onCleanup). A stalled socket freezes pause and seek,
   and closing during a stall hangs the UI thread.
2. **`open` parses the header synchronously on the JS thread**
   (`flux/src/alloy_plugins/video.rs`, `open_impl`).
3. **An underrun corrupts the audio clock.** The sink position is `pushed -
   SDL_GetAudioStreamQueued`; SDL pads an underrun with silence without
   counting it, so the position freezes. After a rebuffer the plane
   re-anchors, then `AudioSync` sees the sound ~450 ms ahead and moves the
   anchor by `AUDIO_SYNC_MAX_SHIFT_US` (125 ms) per frame: Drop/Reanchor cycles
   until the picture has skipped forward.
4. **No async body can feed a worker thread.** `forge::stream::ByteStream` is
   not Send (the alias omits the bound; every producer behind it is Send).
5. **The demuxer trusts a local file.** Four Seek dependencies (the magic
   rewind, `Ebml::skip`, the SeekHead-to-Cues visit at open, `seek`), a size
   read off the wire straight into `vec![0u8; size as usize]` (on the
   armeabi-v7a TV the cast truncates to 32 bits and the cursor desyncs), no
   keyframe gating (a mid-GOP start gives VP9 reference errors, measured),
   errors that are not sticky, and a packet queue that grows without bound for
   a track nobody reads.
6. **Seek is not a no-op on an unseekable source.** `Worker::seek` flushes the
   codec and clears the sink before the demuxer seek fails, then decoding
   resumes on an inter frame.
7. **Mid-stream resolution changes are not handled on the plane.** It
   configures no max-width/max-height and its `present()` ignores
   OutputFormatChanged.

## Design

### 1. A generic streamed byte source in forge

`forge/src/source.rs`, not under `video`. It is the successor of the dev
proxy's range reader, deleted with --proxy-files on 2026-07-21 (one fetch
worker thread, `block_on` per 256 KiB range request, no cancel).

- **Producer trait** (Send): `open(offset)` returns a future of `{ body:
  ByteStream, len: Option<u64>, seekable: bool }`. `ByteStream` gains `+ Send`.
  The one producer in this item is HTTP.
- **Pump.** Producer tasks run on one lazily started forge I/O runtime
  (current_thread on its own thread, alive for the process). Not the host's
  runtime: the flux binaries run current_thread. Because it is never dropped,
  a cancelled DNS lookup (getaddrinfo on the blocking pool) cannot hang a
  close. One CancellationToken per source, tied to player close and engine
  teardown; one reqwest Client per user agent.
- **Reader.** Implements Read + Seek, so it is a `SeekableReader` any existing
  consumer takes. A Mutex ring `{ generation, chunks, total, end: Clean | Error }`
  with a Condvar; the producer re-checks the generation under the lock before
  pushing, so no chunk from before a restart lands after a seek.
  - Seeks are local bookkeeping. A target inside the ring, or a forward skip
    under `STREAM_SKIP_BY_READ_BYTES`, is discarded locally; anything else
    restarts the producer at the offset under a new generation.
    `seek(Current(0))` is a no-op (std and SDL call it). `End` needs a known
    length. A restart on an unseekable source is Unsupported.
  - `interrupt()` wakes a blocked read once, so the reader thread can take a
    command while the body keeps flowing; `close()` cancels the producer.
  - Clean end and error stay distinct: a truncated chunked body is an error,
    never the end.
- **HTTP producer.** `GET` with `Range: bytes=N-`. A 206 with `Content-Range
  a-b/total` is seekable with a length (`a-b/*`: seekable, length unknown); a
  200 is unseekable. Bun 1.3.14 sends Accept-Ranges only on 206 and 416, so
  there is no HEAD probe. Reconnect only on a seekable source, inside the
  producer's retry loop: after any transport error, early EOF or stall, with
  `If-Range` and a check that the new Content-Range starts at the offset with
  the same total (else a "source changed" error), under a named backoff and
  outage budget. An unseekable source never reconnects, since a new request
  restarts it on a new timeline.
- This source bypasses JS `fetch`: no disk cache and no go-client
  `--proxy-http` (the device must reach the host directly). Documented on the
  types.
- Reuse. flux:audio `stream()` cannot take it yet: SDL_mixer decodes on the
  audio device thread, so a network stall would silence every voice. It can
  once clip decode moves to a worker ([[video-playback]] staging item 7).
  forge::cache does not fit (commit-on-complete temp files). Against our own
  servers the dev server's range route reads the whole requested range into
  memory and flux:http has no Range, so a streamed ranged file body in flux is
  the follow-up that lets solidrt serve media too.

### 2. One source rule for media openers

- Defined once in forge, `Source::parse(&str) -> Path | Http`: http or https
  (case-insensitive) is a URL, any other `scheme:` throws (dev validation
  policy), no scheme is a path resolved like `file()`. One flux marshal
  helper applies it.
- Media openers take `string | file()`, matching audio `stream(file())`.
- `SeekableOpener` (an Rc, opened synchronously on the JS thread) becomes a
  Send opener run on the consumer thread, so a file and a URL both open off
  the JS thread.
- createImage and audio keep their meaning in this item; adopting the rule
  later only turns error cases into valid ones.

### 3. The demuxer over a reader plus facts

- `WebmDemuxer::open(reader, facts)`, with `open_path` for tests and the
  texture player. `Ebml::skip` keeps `seek_relative`; the skip policy lives in
  the byte layer.
- No magic rewind: the first 4 bytes are read as the element id.
- Lazy cues: open never jumps to tail Cues (two range requests on the test
  clip, whose Cues are its last 7,542 bytes); the first seek loads them.
- **Duration is a fact only when the Segment has a known size.** An
  unknown-size Segment means `duration` undefined, whatever Duration says.
- **Keyframe gating at every epoch start.** Video is dropped until the first
  sync block K; audio is held in a small preroll ring and kept from `K -
  seek_preroll_us` with the discard point at K, the existing seek rule, so
  Opus converges. Times come from block pts, never cluster timecodes: ffmpeg
  opens every cluster with an Opus block and cluster timecodes sit a few ms off
  the keyframe.
- **Hardening.** `MAX_ELEMENT_BYTES` is checked on the u64 size before any cast,
  for allocations and for skips on an unseekable source. Read errors are
  sticky. A new top-level EBML header ends the stream with `unsupported`.
- `set_tracks(video, audio)`: blocks of a track nobody plays are dropped at
  the source (no sink, an Opus decoder that failed to open).
- `MediaInfo` gains `seekable`, `bt709` and `full_range`; `DecoderFactory`
  takes `&MediaInfo`.

### 4. The reader thread

`forge/src/video/reader.rs`, one per plane player (and later per browser-style
texture player). It owns the byte reader and the demuxer and fills a video and
an audio packet queue in file order.

- **Epochs** have one owner, the reader, and do not care what bumped them (a
  seek here, a live discontinuity in mode 2). Consumers drop older packets.
  Pending seeks collapse to the last one.
- **Head and lead.** `head` is the seek target until the epoch's first released
  frame, then the last released pts; `lead` is the newest demuxed pts in the
  epoch minus head. The published position becomes the target at the seek.
  Measured against the last released pts instead, a forward seek past the
  read-ahead would park the reader for good.
- **Bounds.** `STREAM_READ_AHEAD_US` of lead, and `STREAM_MAX_BUFFER_BYTES` as
  a hard cap.
- **Open** is its first job: the header and the first keyframe, answered on a
  oneshot the binding awaits with `with_pending` (the clipboard.rs pattern).
  `open` rejects on HTTP status, non-WebM, no VP9, `OPEN_TIMEOUT_MS` or abort,
  and the reader stops when the oneshot receiver is gone (engine reload,
  disposed owner). The plane is created after the header resolves, a bounded
  JS-thread wait as today; the one-plane slot is reserved synchronously at the
  call, so two concurrent opens cannot both pass.
- **Published:** newest pts per track, epoch, ended, and the error (a Mutex
  slot).

### 5. Buffering, keyed on something observable

`transport::Shared` gains `buffering`. MediaCodec cannot report that it is
drained, so nothing here waits for that.

- **Enter** when playing, not ended, and lead < `STREAM_LOW_WATER_US`, or the
  audio queue is empty and the sink holds less than `SINK_LOW_WATER_US`. The
  sink is paused while it still holds audio, so its position stays truthful;
  releases hold; the anchor and the audio sync reset.
- **Exit** when lead >= `STREAM_RESUME_BUFFER_US`, or a buffer cap binds, or
  at the end, or on an error. The cap clause keeps a high-bitrate stream from
  deadlocking on the byte cap.
- **Resume in sync.** The first frame after buffering is anchored on the audio
  clock (due = now + pts - audio content time), so a rebuffer does not end in
  an AudioSync correction.
- Starting playback uses the same rule. `STALL_REANCHOR_NS` stays for decoder
  hiccups only.
- The constants' orderings are asserted in a test: read-ahead above resume
  buffer plus `AUDIO_LOOKAHEAD_US`, byte cap above peak bitrate times
  read-ahead, low water above sink low water plus the release lead.

### 6. The plane player

- Consumes the packet queues instead of a demuxer.
- `feed()` peeks the queue before `dequeue_input_buffer`: ndk's InputBuffer has
  no Drop, so a buffer dequeued with nothing to put in it is lost until the
  next flush.
- An input-stall guard like the texture decoder's `STALL_MS`.
- A `lost` sampler passed into `PlanePlayer::open` next to the vsync sampler
  and polled every pass: a lost surface pauses the sink, cancels the source and
  finishes.
- `seek` returns before sending anything when the source is not seekable.
- Drop cancels the source, sends Close and joins the codec worker only.

### 7. JS surface

```ts
open(source: string | FileHandle, options?: { present?, fit?, signal?: AbortSignal })
player.seekable: boolean          // seek is a no-op when false
player.buffering(): boolean       // plain read, like playing/currentTime/finished
player.error(): VideoError | undefined   // plain read
// VideoError: an Error with kind "network" | "decode" | "unsupported" | "no-plane"
// duration: undefined when the source has no known end
// width/height: the size at open
```

- A rejection and `error()` carry the same Error object. `finished()` keeps
  its meaning (the last frame was displayed) and is not set by a failure.
- `buffering` rather than the web's `waiting`: the web has only an event for
  this state, the project already names states in plain words (`finished`, not
  `ended`), and buffering is the word player APIs use for it.
- Core `createVideo` aborts `signal` in onCleanup so a pending open stops,
  keeps one reactive `error()` for open and mid-stream failures (fed by a
  native one-shot "failed" promise), and adds `seekable()` and `buffering()`.
  An app's plane-to-texture fallback keys on `kind === "no-plane"`, so a
  network error does not reopen the stream through a texture.

## Not in this item

- **The texture player.** Its behaviour is unchanged: local files through its
  current worker and frame-loop clock, no seek. It takes only the demuxer's
  signature changes. A URL with `present: "texture"` rejects with kind
  `unsupported` ("streaming plays on a plane until the texture player moves
  off the frame loop"), and it reports the same fields (`seekable: false`,
  `buffering()` false). The texture player's own trajectory is the
  browser-style rework in [[android-video-punch-through]] ("The texture path,
  browser style"): a decode worker releasing (frame, due time) to a
  raster-thread latch, built on this item's byte source, reader and transport.
  URL sources, buffering and seek reach the texture player with that rework,
  so nothing here is spent on the current frame-loop worker or its clock.
- **Mid-stream resolution change and adaptive streaming.** Detected
  (OutputFormatChanged with a new size on the plane) and ended with
  `unsupported`. Later, additively: max-width/max-height at configure with a
  plane aspect update, an id-stable YUV resize on the texture path, and plain
  reads `videoWidth()`/`videoHeight()` (the web names).
- **flux:audio `stream()` over the byte source**, after its decode moves to a
  worker; createImage adopting the source rule.
- **A streamed ranged file body in flux** for the dev server and flux:http.
- Shipping: behind the `video` feature, dev builds only; lifting the dist gate
  stays with [[video-playback]].

## The test app after

- `projects/video-streaming/src/index.tsx`: no copy step;
  `createVideo(url, { present: "plane", autoplay: true })`; a buffering
  indicator (a static one is free over a plane, an animated one runs near
  16 fps there until [[plane-adaptive-fence-wait]]); the fallback on
  `error().kind === "no-plane"`. Off Android the texture fallback shows the
  `unsupported` message until the texture rework.
- `server/stream.ts`: the clip stays served as a file (`Bun.file`), which Bun
  already answers with Range and 206; the one change is `content-type:
  video/webm` (it sends `video/mp4`).
- Later, when a server should decide what it sends (transcoding another format,
  starting at a time), an ffmpeg feed works. Measured 2026-09-13 on Bun
  1.3.14:
  - **Pace the feed.** Bun applies no backpressure to a streamed body (the
    whole ffmpeg output lands in server memory at about 2.4x its size, in
    every body shape tried), so `-re` is the only memory bound.
  - **Give the client a buffer.** Plain `-re` keeps the client's lead at
    0.05-0.44 s; `-readrate_initial_burst 5` holds it near 4.9 s. The player
    then also needs a resume rule for a source delivering at realtime (resume
    once the lead stops growing above the low water).
  - **Timeouts and teardown.** `server.timeout(req, 0)`, since the default idle
    timeout cuts a connection with no bytes moving after about 12 s; kill
    ffmpeg on `req.signal` abort.
  - **Seek** is an app-level reopen at `?t=`, with `-ss` before `-i` and
    `-copyts` so the pts carry source time.
  - **Cost.** A 1080p VP9 realtime transcode ran at 4.4x realtime at
    `-cpu-used 8`.

## Order of work

One stage, streaming on the plane, built in this order:

1. The byte source and the demuxer changes, with their forge tests.
2. The reader thread and buffering in transport, with forge tests against a
   stalling scripted server.
3. The plane on the reader, async open with abort, the source rule, the JS
   surface.
4. The test app and the device runs.

The texture player's browser-style rework builds on steps 1 and 2.

## Done looks like

- The test app on the TV plays the served clip from its URL with no copy, and
  the VIDEO layer census reads as in [[android-video-punch-through]] (every
  interval two periods for 25 fps on the 50 Hz panel).
- A seek lands within one keyframe interval of its target.
- A server stall of a few seconds gives one buffering episode, and playback
  resumes in sync on avsync.webm with no anchor move and no dropped or
  re-anchored frame in the log.
- Close and engine reload return promptly during a stall and during a pending
  open.
- Memory stays under `STREAM_MAX_BUFFER_BYTES`.
- `open(url)` on a texture player rejects with `unsupported`.
- The same on the tablet.

## Verification

- forge tests:
  - byte source: a seek racing a chunk the producer holds while blocked on a
    full ring; `interrupt()` waking a blocked read while the body flows; a
    truncated chunked body reading as an error; a reconnect validated against
    Content-Range; close during a stall, during DNS and during a pending open
    (the scripted server in `forge/src/tests/fetch.rs`).
  - demuxer: one byte per read, fixtures rewritten to unknown sizes, truncation
    at every byte of the last cluster, garbage sizes (an error, no allocation,
    no silent skip), keyframe gating with audio preroll, an unknown-size
    Segment hiding Duration.
  - reader: a forward seek past the read-ahead; an unplayed audio track.
  - transport: buffering entering and exiting without an AudioSync
    correction; the constant orderings.
- Device, the TV first and the tablet second, reading the SurfaceFlinger
  census, logcat and memory as in [[android-video-punch-through]].

## Findings

- Step 1 (`forge/src/source.rs`, 2026-09-13). Two departures from the design
  text above. The reconnect loop lives in the pump, not the producer: the
  pump knows `seekable`, the stall limit and the backoff, and a producer only
  has to refuse a reopened source that is not the one it first saw (an
  `InvalidData` error, the one kind the pump never retries). And
  `interrupt()` surfaces as `WouldBlock`, not `Interrupted`: std's
  `read_exact` swallows `Interrupted` and blocks again, so the demuxer would
  never see it.
- The ring is a transfer buffer with its own cap (`STREAM_RING_BYTES`);
  `STREAM_MAX_BUFFER_BYTES` stays the reader thread's packet cap.
- `tokio_util::sync::CancellationToken` needs no cargo feature in 0.7.19.
- Step 1, demuxer half (`forge/src/video/webm.rs`, 2026-09-13). `seek`
  returns the resume position (the target, or the epoch's first keyframe
  when that comes later) and `MediaInfo` gains `start_us` (the first
  keyframe at open), because keyframe gating moves where playback resumes
  and the players' audio discard point has to move with it: the plane skips
  video and discards decoded audio up to that one position. A read
  interrupted by the source (`WouldBlock`) is the one error that is not
  sticky: it leaves the walk mid-element, and the only valid next call is a
  seek. ffmpeg writes each track's last packet as a BlockGroup, so a trailing
  BlockGroup is not necessarily video. The new fixture `video_gop.webm`
  (clusters every 500 ms, keyframes every second, tail Cues) is what the
  gating and lazy-cues tests need; `video_av.webm` has a single cluster.
- Steps 2 and 3 (`forge/src/video/reader.rs`, `transport.rs`, `plane.rs`,
  `flux/src/alloy_plugins/video.rs`, 2026-09-13), built together since the
  reader is only exercised through the plane. Departures and findings:
  - Errors are typed at the source: `forge::video::StreamError { kind,
    message }` replaces the demuxer's and players' `String` errors, so the
    JS `kind` is decided where the failure happens (the EBML cursor knows an
    io error from a malformed size) instead of by matching messages. A read
    interrupted by the reader's own command is a kind of its own
    (`Interrupted`), never published.
  - The demuxer gained `next_packet` (both tracks in container order), which
    is what the reader fills its queues with; `next_video`/`next_audio` stay
    for the texture player and the tests.
  - The reader's consumer never blocks and never finishes on a failure:
    `Next::{Packet, Waiting, End}` from the queues, and the error, the
    epoch's resume position, the lead and the byte cap from `status()`. The
    plane player feeds no audio until the epoch's resume position is known,
    so the audio discard point is the keyframe the demuxer actually landed
    on, not the target.
  - A seek whose target opens a cluster gets no audio preroll: the preroll
    packets sit in the cluster before, which the seek does not visit (a cue
    points at the keyframe's cluster). Opus converges within its first
    packets, so the cost is a slightly wrong first 80 ms after such a seek;
    reading the previous cluster would cost a whole second of data.
  - The audio-starvation buffering rule applies only short of the resume
    buffer, otherwise it flaps against the exit rule on a stream whose audio
    track ends before its video.
  - The mid-stream failure reaches core through a one-shot `failed` promise
    on the player (a plane worker has no JS-thread tick to poll from); the
    texture player's never settles. `error()` is sticky: a seek after a
    failure retries the source, but the record stays.
  - `SeekableOpener` is `Arc<dyn Fn + Send + Sync>` now; `file()` objects
    open on the reader thread through it.
  - The Android half of flux cannot be type-checked here: `cargo ndk check`
    builds forge (with `plane.rs`) but stops at sdl3-sys for flux, so the
    plane binding in `flux/src/alloy_plugins/video.rs` was reviewed by hand
    and waits for the first device build.
- Step 4, first device build and tablet runs (2026-09-13):
  - The first Android build failed in the flux plane binding: the player's
    `error` method, a closure capturing the player id and returning
    `Value<'js>`, hit the lifetime trap in flux/CLAUDE.md ("Ctx and the 'js
    lifetime"). A `value_builder` coercion helper fixed it.
  - A plain server pause cannot produce the stall check: the tablet held
    about 10 MB beyond the playback position (the 6 s read-ahead, the 1 MiB
    ring, TCP buffers), and avsync.webm is 145 KB in all. The check needs a
    source that sends at playback pace.
  - Re-anchors were silent (`Release::Reanchor` in `plane.rs`), and dropped
    frames are only counted when playback finishes; the plane now logs a
    re-anchor at info. No forge info line reached a client's log anyway:
    alloy's logger passed every level only for the alloy, flux and lattice
    targets and cut the rest, forge included, to warnings. forge is on that
    list now.
  - The first seek over HTTP failed for good. `Reader::seek` always
    interrupts the source, but a reader parked at its read-ahead bound has
    no read to wake, so the sticky interrupt failed the seek's own first
    read, the jump to the tail Cues. `load_cues` had already taken the one
    Cues position, so it gave up and the seek failed as `unsupported`. The
    reader loop now clears a pending interrupt when it takes a command (a
    command and its interrupt are queued under the same lock), and an
    interrupted `load_cues` keeps the position and returns the interruption.
    The existing seek test opens a local file, which has nothing to
    interrupt; `a_seek_after_the_thread_parked_reaches_the_tail_cues_over_http`
    covers HTTP and fails without the fix.
  - Review of that fix found two more holes on the same path, neither hit
    on the devices. A seek queued behind a track change lost its
    interrupt: the track change's pop cleared it and fell through to the
    next read, which blocks on a stalled body until the stall ends; every
    command now continues the loop, so the queue drains before a read. And
    a read the source interrupts part-way (blocked on a stalled body)
    consumes bytes the demuxer's walk never counted, so the relative seek
    that followed landed past its target ("seek head does not point at
    cues"); `Ebml::seek_to` now measures the jump from the inner reader's
    real position. `a_seek_queued_behind_a_track_change_is_taken_during_a_stall`
    covers both and fails without either.
  - Resumes after buffering were not in sync. The first frame set the
    anchor to the paused sink's content time plus `AUDIO_OUTPUT_LATENCY_US`,
    and `correct_clock` ran on that frame before `start_audio` shifted the
    latency back off: it read a lead of exactly -60 ms against the 40 ms
    threshold and moved the anchor at once. Play after a pause showed a
    single move too, -45 to -84 ms: after a paused seek the next frame is
    one frame past the target the sound starts at, and the paused SDL
    audio device takes a moment to consume again. Skipping the observation
    while the sink was paused would only have traded one move for the other.
  - So every start now waits for the sound. The first frame after play, a
    seek or buffering starts the sink while that frame is still in hand,
    waits until the sink's position moves (at most `AUDIO_START_TIMEOUT`,
    300 ms; nothing queued or a command ends the wait at once), and anchors
    on the sound's content time, which is net of the output latency. That
    one path replaces the separate buffering resume and the latency shift
    after the first release. On the tablet the first start, play after a
    pause, a paused seek then play, a seek while playing and both stall
    resumes then logged no anchor move, no re-anchor and no timeout.
- Step 4, TV sync run (2026-09-13, armeabi-v7a, the tablet's code):
  - The first playback right after installing the client stumbled once in
    its first second: two audio-sync anchor moves (-113 ms, -71 ms), a frame
    at 0.64 s re-anchored 418 ms late and 2 late frames dropped, with the
    picture up 2.3 s after the open. The next two opens, and a relaunch of
    the client followed by an open, started cleanly, so it most likely was
    the first run after an install warming up rather than the start path.
  - The replay's census had one 3-period interval followed by a 1-period
    one: a frame a vsync late and the next a vsync early, nothing lost. The
    first run's window was all two-period intervals.
  - The remaining checks passed as on the tablet (seeks, the unseekable
    seek, the 12 s stall's reconnect, close and reload, the hanging open),
    with two differences. A close during a stall held the JS thread about
    1 s, against about 0.4 s for a close during playback (and 0.36 s on the
    tablet during a stall); which teardown step takes the extra time is not
    known yet. And the first starts of crawl and of the unseekable avsync
    each logged one anchor move, -40.1 and -40.3 ms, just past the 40 ms
    threshold, while the stall resumes, seeks and other starts logged none.
    Likely cause: the start anchor is taken right when the sink position
    jumps by a whole device buffer, larger on the TV than on the tablet,
    which leaves the first frames early by about half a buffer.

## Mode 2 (deferred): what it adds, and what this item holds for it

Already here, so mode 2 is additive: the source union and the producer
trait; the `seekable` fact with a structural no-op seek; cause-agnostic epochs
with keyframe gating at every epoch start; clean end distinct from error in
the byte and demux layers; the first-frame anchor (the plane's now, the texture
player's after its rework); `duration` undefined without a known end;
`finished` and `error` defined for a stream with no known end.

Mode 2 adds:

- **API:** an explicit option, `latency?: number` (seconds behind the live
  edge). Present means live policy, absent means this item's behaviour.
- **Transport:** latency judged on the lead's minimum over the last arrival
  window, because a producer that delivers whole clusters makes the lead a
  saw-tooth; catch-up by dropping to a keyframe, handled as an audio seek
  (clear the sink, reset Opus, preroll); a live pause that resumes at the live
  edge.
- **Reader:** reconnect and rejoin as a fresh demuxer plus an epoch bump; a
  new EBML header as a discontinuity, continuing only when codec, size and
  audio layout match; resync to the next Cluster after a byte gap (a join
  without a relay).
- **Producers:** a fetch Response body and subprocess stdout directly (a Send
  ByteStream), and p2p, net and websocket through thin adapters over their
  Send halves; `source` widens to those handles.
- **Server:** a relay that caches the bytes before the first Cluster and starts
  each client at a cluster whose first VIDEO block is a keyframe (in pipe
  output no cluster starts with a video block), or one ffmpeg per client
  (`-listen` serves a single client).

Related: [[android-video-punch-through]], [[video-playback]],
[[plane-adaptive-fence-wait]].
