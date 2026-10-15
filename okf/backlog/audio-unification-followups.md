---
title: Audio unification follow-ups - what the mixer work surfaced
description: Loose ends and improvements met while replacing SDL_mixer with the own mixer (okf/done/audio-unification.md, 2026-10-15), kept in one file at the user's request; split an item out when it is picked up.
created: 2026-10-15
---

# Audio unification follow-ups - what the mixer work surfaced

One file, by request, so the mixer session's observations are not lost.
Each entry is small on its own; pick one, give it its own item if it grows.

## Clock and sync

- **The device's own latency is invisible.** SDL has no output timestamp,
  so `audioDeviceQueuedMs` (SDL's queue) reads 0 everywhere while the TV
  rendered the first ~140 ms of a cold device ahead of real time, and the
  speaker path below SDL is what `AUDIO_OUTPUT_LATENCY_US` guesses at (60
  ms, TV-tuned). AAudio has `getTimestamp`, CoreAudio and WASAPI the like;
  an SDL-side wrapper in `sdl_utils.rs` would need SDL to expose it first
  (upstream ask), else the constant becomes the app setting the
  `forge/src/video/audio.rs` comment already names.
- **`PcmSinkHandle::position_at_us` is unused.** The callback publishes
  (position, time, block length) per block for interpolation; video's
  `AudioSync` still reads the block-quantized position. Wiring it in is
  video-playback's open stage 3 (audio-clock smoothing).
- **`ended()` flips at render time**, the device buffer ahead of the
  speaker; documented. A speaker-time `ended` would hold the slot until
  the device has consumed the voice's last frames (the lead is known per
  block); decide whether any consumer needs it before building it.
- **Clip positions step per block** (23 ms on the devices). The same
  per-block stamp interpolates them too, should visuals synced to
  `position()` want finer than a block.

## Mixer

- **FEED_FRAMES (512) backlog.** A voice pulls up to 512 source frames
  into its converter per feed, so a sink's `clear` takes effect in the
  mix within a block but a reader's view lags that block. Feeding exactly
  what the block needs (SDL's `available()` is an estimate while
  resampling) would shrink the lag; measure before changing.
- **SDL allocates inside `SDL_PutAudioStreamData` when a stream's pool
  grows**, on the audio thread. Unverified whether the pool stays sized
  after the first blocks; a pre-size at `play()` (one put and clear) is
  the fix if a trace shows allocation per block.
- **Bus names live for the process** (`MAX_BUSES` 32, never freed). An
  app that invents bus names per level hits the cap in a long session;
  free a bus when it has no voices and its gain is unity, or document.
- **Underruns are counted per block.** A stat per ring (which voice
  starved) would name the pusher; today the count is global.
- **Streams seek only via a restart.** `stream.rs` has the restart
  machinery at any frame; a `Playback.seek(seconds)` for streamed (and
  decoded) playbacks is a command away.
- **`stream()` over HTTP is now possible** (decode on its own thread, a
  stall starves one voice): the video-streaming plan's reuse note.
  `forge::audio::open` takes any `SeekableReader`; the flux side needs
  the one-source rule and the `Reader` from `forge::source`.
- **Device hot-plug and format change** are SDL's (the stream follows
  the default device) and untested here; the device rate is fixed at
  open, so a default device at another rate resamples in SDL from then on.
- **Sources with more than two channels are refused**; the mix is stereo.
  Surround output is SDL's downmix of our stereo. Fine until a consumer.

## Tests and tooling

- **The test harness cannot step the mixer's clock.** Under `sol test`
  app time is frames, the mix runs on the device; the audio app test
  asserts only what answers at once. A test audio device driven by the
  frame clock (render N frames per `advance`) would make fades, loops and
  sink positions testable deterministically; it is the "test source
  counterpart" audio-pcm-sink-js left open.
- **Probe asset convention.** The repo-root `assets/` folder (untracked)
  is where probes find their files (`assets/audio/` holds the four audio
  fixtures now); nothing documents it. Either document it in
  `packages/cli/agents/debugging.md` or give probes a tracked folder.
- **`sol run --client 0`** did not suppress the local client (a `--lan`
  run for devices still opened one). Check the flag's meaning and doc.
- **No local prettier** in the checkout and `bunx` is off limits, so the
  TypeScript files of this work were not formatter-checked.
- **`forge/examples/`** exists untracked with no files of this work;
  something else's leftover.

## Formats and content

- **`.ogg` imports lost their type declaration** (`packages/cli/assets.d.ts`
  now declares `.opus` and `.weba`); `sandbox/audio/assets/*.ogg` are
  Vorbis and no longer play. Re-encode them (`ffmpeg -c:a libopus x.opus`)
  or delete the sandbox app.
- **Keep or drop the Vorbis fixture?** `forge/src/tests/data/audio_vorbis.ogg`
  exists to pin the "encode as Opus" refusal message. The user was uneasy
  about a Vorbis file in the tree; decide, and drop the message's test
  with it if it goes.
- **A `sol` bake for audio** (wav to .opus or .weba, as the video bake)
  would give apps the small-file path without ffmpeg; an own muxer can use
  a finer WebM TimestampScale than ffmpeg's 1 ms (Findings: that rounding
  is why a seek lands up to a tick off).
- **Ogg Opus end trim and pre-skip are libopusfile's; WebM's are ours**
  (`audio/trim.rs`, DiscardPadding). Two paths for one codec; fine as long
  as both are tested, which they are.

## Platform

- **32-bit Android stdio**: opusfile needed `_FILE_OFFSET_BITS 32` on
  armeabi-v7a (API 21). Any further vendored C library using
  `fseeko`/`ftello` under 64-bit offsets will hit the same; the build.rs
  comment is the place to point at.
- **The TV's texture video path** skipped 1 of 150 frames with sound on
  (none late); known from video-playback (fullscreen 1080p over budget),
  not new.
- **The worker's end detection** runs on its idle poll (250 ms);
  `AudioSink::end()` made that harmless for the stats, but a sink paused
  only on the next poll still consumes silence for up to that long.
