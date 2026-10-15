---
title: Audio unification findings - the own mixer, the decoders, the devices
description: What building the SDL-free mixer and the Opus/WAV decoders taught (2026-10-15): WebM DiscardPadding and ffmpeg's timestamp rounding, converter backlog and ring clears, stop semantics against a callback, device blocks and the TV's lead below SDL, 32-bit Bionic stdio, the test harness against a device clock.
created: 2026-10-15
---

# Audio unification findings - the own mixer, the decoders, the devices

Cut from [audio-unification](../done/audio-unification.md) at completion.

- WebM carries Opus's end trim as `DiscardPadding` (0x75A2, a signed
  nanosecond count in the last packet's BlockGroup; ffmpeg writes 648
  samples on a 0.5 s sine). The reader ignored it until 2026-10-15, so a
  WebM audio track played its encoder tail: ~13 ms of garbage at the end
  of every video with sound. Honoured now on both the clip decoder and
  video's audio track, through `AudioPacket::discard_padding_us`.
- ffmpeg's WebM muxer writes Opus block timestamps on its 1 ms
  TimestampScale with two roundings (the packet time to the tick, plus the
  CodecDelay of 6.5 ms rounded to 7): blocks sit at 0, 21, 41, 61 ms for
  samples at 0, 20, 40, 60. A seek that trims by the container's timestamps
  (the spec's rule: subtract CodecDelay) therefore lands up to one tick (48
  frames) off; ffmpeg seeking its own file does the same. Start and end
  trims are sample-counted and exact. An own bake (wav to webm) can use a
  finer scale.
- ffmpeg's `-ac 2` upmixes mono to stereo at -3 dB; a fixture whose
  channels must equal the mono source needs `-af pan=stereo|c0=c0|c1=c0`.
- libopusfile buffers a packet's decoded samples internally and hands out
  what fits, so `op_read_float` works with any buffer size; its HTTP half
  is opt-in (`OP_ENABLE_HTTP`), so compiling `http.c` without it yields the
  stubs `opusfile.c` links against and nothing else.
- A voice pulls source frames into its converter up to 512 at a time
  (`FEED_FRAMES`), so a ring can read empty while the converter still
  holds its tail. Everything a reader sees is therefore stated against the
  mix, not the ring: position = fed less the converter's backlog (kept
  exact from output frames times the ratio), queued = ring plus backlog,
  and position + queued = pushed, today's `pushed - queued` identity. The
  backlog is published per block, so a reader's view lags a clear by at
  most one block.
- A sink's clear must be applied at the top of the voice's block, not where
  the ring is read: a converter already holding a block's worth is never
  asked to feed, and the first version played the cleared audio out of it.
  The ring carries a generation counter the voice compares every block.
- A stream voice started for a restart waits for the ring's generation to
  reach the restart's before it reads, so it never plays what the restart
  discards; the wait counts as no underrun (a ring that has not played yet
  is filling, not outrun).
- A replacement voice over the same ring (a stream played again) starts
  from the ring's current index: what the stopped voice had pulled ahead
  goes with it, as a stop discards. Command order (stop, then start, in one
  batch) keeps the two from ever reading the ring together.
- SDL_mixer destroyed a stopped track synchronously, so `stop()` then
  `play()` in the same tick never saw the voice cap; with the callback
  retiring voices a block later, the probe's 256-voice test tripped the
  cap on the very next play. The control side now marks an immediately
  stopped track ended at once (so `ended()`, the cap and the sweep agree)
  while the voice table carries a block's worth of headroom above the cap
  for slots still retiring.
- Desktop (PipeWire through SDL, 48 kHz): the device block the callback
  renders is 1024 frames, 21.3 ms, seen as the first jump of a video
  sink's position. The audio-clock quantization video-playback knows is
  that block; the interpolated `position_at_us` is what would smooth it.
- Verified 2026-10-15 on the desktop client: probes/audio-mix-probe.tsx
  26/26 after stage 3 (rate, fades, ramps, master ramp, validation, voice
  cap, bus stop, bus fade, global fade, bus gain, pause/resume holding the
  position, bus pause, duration, position at end, setLoop ending after
  the pass, a sink's push/queued/position/pause/clear/close),
  probes/audio-decode-probe.tsx 8/8 (WAV load, .opus stream
  play/replay/loop, WebM stream and load, Vorbis and garbage refused by
  name), examples/video plays with its audio track on the mixer (150
  frames latched, none skipped or late), `/stats` carries audioVoices,
  audioUnderruns, audioRenderMaxMs (0.1 ms on the desktop), the device
  rate and block.
- The test harness steps app time by frames; the mixer renders on the
  device's clock, which no test controls. An app test under `sol test`
  therefore asserts what the surface answers at once (duration, a fresh
  position, ended on stop, a sink's queue after a push, validation) and
  leaves timing to the probe on a real client
  (packages/core/tests/audio.test.tsx says so in its header). A device
  clock the harness can step is the open design question
  audio-pcm-sink-js named; it did not move here.
- Tablet (SM-T500, Android, AAudio through SDL): 44.1 kHz device, 1024
  frame blocks (23 ms); probes/audio-mix-probe.tsx 26/26, the decode
  probe 8/8, worst render pass 5 ms during the 256-voice cap test, 1.3 ms
  with video playing. The arm64 cross build compiled libogg (cmake) and
  libopusfile (cc) under cargo-ndk unchanged.
- The video worker left its sink running after the track ended, so the
  mixer counted an underrun every block for as long as the player sat at
  its end (1956 on the tablet after 45 s of a finished 6 s clip). The
  worker now stops the sink once the track has ended and drained; the
  counter means starvation only.
- armeabi-v7a (the TV): opusfile's `internal.h` sets `_FILE_OFFSET_BITS
  64`, under which 32-bit Bionic declares `fseeko`/`ftello` only from API
  24 (as `*64` renames), and the target is API 21; clang 16+ makes the
  implicit declaration an error. The stdio path they serve (`op_fopen`)
  is unused (the reader goes through callbacks), so forge's build.rs
  compiles opusfile with 32-bit offsets on 32-bit Android.
- TV (Philips TPM171E, armeabi-v7a, 44.1 kHz, 1024-frame blocks): right
  after the device opens, a half-second clip at rate 2 read as ended after
  107 ms, i.e. the first ~140 ms of audio were rendered ahead of real time
  into the platform's audio stack; later runs on the warm device showed
  no such lead. `audioDeviceQueuedMs` (SDL's own stream queue, now in
  `/stats`) reads 0 on every device, so whatever the platform holds below
  SDL is invisible to it, as the AUDIO_OUTPUT_LATENCY_US comment already
  says (no output timestamp API). Positions are nevertheless published net
  of SDL's queue (read at the start of each callback), which restores the
  SDL_mixer-era sink meaning, "consumed by the device", exactly, wherever
  SDL does hold frames. `ended()` flips at render time, documented; the
  probe measures the rate against a rate-1 render of the same clip rather
  than a clock.
- Even with the worker stopping a drained sink, a finished video track
  counted ~11 underruns on the desktop: once its packets ran out, the
  ring drained block by block until the worker's next idle poll noticed
  (~250 ms). The pusher knows the sound is over, so the contract says so:
  `AudioSink::end()` (a default no-op on forge's trait, called when the
  track's packets end) sets the ring's ended flag, and a sink voice on an
  ended, empty ring renders silence without counting; a `clear` (a seek)
  restarts it. Underruns on the video path then mean starvation only.
- A playback's `position()` and `ended()` answer through a reader on the
  voice's shared state (`AudioTrackReader`), not the registry: the
  registry forgets a track once it ended, and a playback object outlives
  that.
