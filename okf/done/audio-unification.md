---
title: Audio unification - one mixer for clips, streams, sinks and video
description: Replace SDL3_mixer with an own mixer rendered in SDL's device callback (sample-accurate gain, pan and fade automation, buses with gain, pause and position, loops without seams) fed by engine-free decoders in forge (WAV own, .opus via vendored libopusfile, WebM/Opus via the existing reader), so clip audio, streamed tracks, PCM sinks and video audio share one PCM path under one master; implements the declared setBusGain, pause/resume, position, duration, setLoop and the JS PCM sink.
created: 2026-10-15
completed: 2026-10-15
---

# Audio unification - one mixer for clips, streams, sinks and video

Staging item 7 of [video-playback](../backlog/video-playback.md), shaped
2026-10-15. Closes the open halves of
[flux-audio-mix-control](../done/flux-audio-mix-control.md) (bus gains,
position/duration, setLoop, pause/resume) and
[audio-pcm-sink-js](../backlog/audio-pcm-sink-js.md).

## Why

`alloy/src/audio.rs` runs two audio worlds. Clips go through SDL3_mixer:
per-track gain, stereo gains and frequency ratio, all immediate writes, so
ramps are a 100 Hz thread (`sol-audio-ramp`) stepping SDL setters through
raw pointers; tags group tracks but `MIX_SetTagGain` overwrites each
track's own gain, so `setBusGain` is declared and throws; decoding runs on
the device thread, so a stalled source would silence every voice. Video
audio and the planned JS sink are plain SDL3 streams outside the mixer:
`setMasterGain`, buses and `stop()` never reach them, and "mute the app" is
not expressible. Every workaround built around SDL_mixer is thrown away the
day it goes, and the audio surface already promises things it cannot do.

## Decisions

- **Own mixer in SDL's get-callback**, not one SDL stream per voice with
  SDL mixing (the backlog's sketch). SDL3 streams have gain and frequency
  ratio and nothing else: no pan, no fades, no loop, no pause with
  position, no bus. Applying those on push puts them a lookahead behind
  the speaker, and ramps would still need the control-rate thread. Every
  engine (FMOD, Wwise, Web Audio, miniaudio) renders its own mix in the
  device callback with per-sample automation; so does this one.
- **SDL stays for what it is proven at**: the device (open, migration to a
  new default device, the callback thread) and the resampler: one unbound
  `SDL_AudioStream` per voice as a pure converter (clip rate and channels
  in, device rate stereo out, `SDL_SetAudioStreamFrequencyRatio` for the
  playback rate). No own resampler.
- **Decoding in forge, engine-free, no new crates.** `forge::audio` holds
  the clip decoders behind one trait: WAV (own parser), `.opus` (Xiph's
  libopusfile with libogg, vendored beside `forge/vendor/opus`, built by
  `cc`, thin FFI module plus a safe type: the lane libopus and libvpx are
  in), and WebM/Opus (the existing reader plus the vendored libopus). A
  headless `flux` script can then decode audio without alloy, the way
  `forge::image` decodes images; a `decodeAudio` module is a later thin
  marshal, not part of this item.
- **Formats: Opus and WAV.** Vorbis and MP3 are out; nothing shipped plays
  them (three scratch `.ogg` tracks in `sandbox/audio` re-encode to Opus).
  symphonia was considered and dropped: with Opus the codec story is one
  decoder across video and audio, and WAV needs no decoder at all. If a
  consumer for another codec appears, that is its own item.
- **WAV is ours**, about 150 lines over a thirty-year-old specification:
  chunk walk with word-aligned padding, `fmt ` including
  `WAVE_FORMAT_EXTENSIBLE`, PCM 8/16/24/32 and float32 to f32, mono and
  stereo, compressed variants and RF64 rejected by name. `hound` was the
  acceptable fallback; SDL's `SDL_LoadWAV` was rejected for the layering
  (it would put a decoder in alloy).
- **Units**: seconds for `position()` and `duration`, as the video
  player's `currentTime()` and `duration`; options stay in milliseconds
  (`rampMs`, `fadeOutMs`, `fadeInMs`) as today. `duration` is
  `number | undefined`, undefined when the container cannot say, the video
  player's shape.

## Layering

- `forge::audio` (new): `Pcm { sample_rate, channels, samples: Vec<f32> }`
  (interleaved), the `AudioDecoder` trait (`info()` with rate, channels and
  `n_frames: Option<u64>`; `read(&mut [f32]) -> frames`; `seek(frame)`),
  the three decoders, `open(reader) -> Box<dyn AudioDecoder>` sniffing the
  magic (`RIFF`/`WAVE`, `OggS`, the EBML id; Ogg Vorbis gets its own
  message naming Opus), and `decode(bytes) -> Pcm`. The pre-skip and
  seek-preroll trim rule that `forge/src/video/audio.rs` applies to video's
  audio track becomes a shared helper both use; the video player's
  behaviour does not change.
- forge features: `audio` (the libopus, libogg and libopusfile builds; the
  WebM reader is pure Rust and needs no gate) and `video = ["audio", ...]`.
  The lattice goals already enable `video`.
- `alloy::audio`: the mixer and the device. Takes `Pcm` and
  `Box<dyn AudioDecoder>`, knows no format. The public `Context` methods
  keep their names (`load_sound`, `play_sound`, `set_audio_gain`,
  `stop_bus_audio`, `create_pcm_sink`, ...) and gain the new ones, so the
  flux plugins change where the surface changes and nowhere else.
- `flux/alloy_plugins/audio.rs`, `video.rs`: marshal only.
- `forge::video`'s `AudioSink` trait and the player: untouched. Alloy's
  sink handle implements it as before.

## The mixer

### Threads

The JS thread issues commands. SDL's device thread runs the get-callback
on the one device stream; that callback is the mixer. A lazily spawned
`sol-audio-decode` thread feeds streaming voices. The `sol-audio-ramp`
thread, its raw-pointer table and the `MIX_*` wrappers go.

### Realtime discipline

In the callback: no allocation, no blocking lock, no I/O, no SDL object
creation or destruction, no logging. Concretely:

- Commands travel in a `Mutex<Vec<Command>>` the JS thread pushes to; the
  callback `try_lock`s and swaps it against its own empty, preallocated
  Vec, applies the commands, and swaps the drained Vec back on the next
  block. A failed `try_lock` (the JS thread mid-push) delays commands by
  one block. The producer side may grow the Vec; the consumer never
  allocates.
- Retired voices (ended, stopped, swept) travel back the same way, so the
  per-voice converter streams are created in `play()` and destroyed by the
  sweep, both on the JS thread.
- Voice table: `MAX_LIVE_TRACKS` (256, today's cap) fixed slots, each
  addressed by `(slot, generation)` so a stale command never touches a
  reused slot. Scratch buffers are sized at device open for the device's
  block and rendered in chunks when a migrated device asks for more.
- State JS reads (`ended`, `paused`, the position cursor) are atomics in an
  `Arc<VoiceShared>` the registry entry holds: lock-free reads, updated
  once per block.
- Rings between a producer thread and the callback are an own bounded SPSC
  ring (two atomic indices, one producer, one consumer), tested on its own.
  A sink's `clear` is producer-side: the producer publishes a discard mark
  at its write index; the consumer skips to it and counts the skipped
  frames as consumed. No command round trip, so a push right after a clear
  (the video worker's seek) is never swallowed.
- SDL's stream queue allocates when it grows. Each converter is pre-sized
  on the JS thread at `play()` (one put and clear of a maximal block) so
  steady-state puts in the callback reuse SDL's pooled memory. Whether SDL
  pools as expected is checked, not assumed (Findings).

### Rendering a block

For each live voice: pull source frames into its converter until the
converter can supply the block (wrapping at the loop point for a looping
clip, so the resampler interpolates across the seam and a loop at any rate
cannot click), set the converter's frequency ratio from the rate ramp (a
block parameter, as SDL's ratio is), get the converted stereo frames, then
per sample: `voice.gain x envelope x bus.gain x master x pan`, accumulate
into the mix. A non-looping clip flushes its converter at the end so the
resampler's tail drains; the voice retires when the converter is empty.
The mix is written to the device stream with one put.

- **Gains** are three independent linear ramps (from, to, frames left)
  evaluated per sample: the voice's own gain, its bus's gain, the master.
  A new set takes over from the ramp's current value; an immediate set
  cancels the ramp. Sample-accurate, where the 100 Hz thread stepped.
- **Envelope**: the fade-in/fade-out ramp with an end action (retire), kept
  apart from the user gain so a `setGain` during a fade does not fight it.
  `stop({ fadeOutMs })` ramps the envelope from its current value to 0 and
  retires; `fadeInMs` ramps it 0 to 1.
- **Pan** keeps today's law and SDL's behaviour exactly: the converter
  upmixes mono to stereo, pan gains apply per channel after, so mono is
  placed equal-power, stereo is balanced, and an unpanned voice gets no
  gains at all (the documented ~3 dB).
- **Pause** is a voice flag or a bus flag: paused, the voice renders
  nothing and its cursor stands. A voice started on a paused bus starts
  paused (bus state applies to live and future voices, as bus gain does).
- **Position** is the source cursor in clip frames, published per block;
  `duration` is the clip's frame count.
- **Buses** are indices in the callback; alloy owns the name map (created
  by first use, `MAX_BUSES` named constant, never deleted). The plugin
  tracks which bus gains an engine set and resets them with the master on
  teardown.

### Voice sources

- **Clip**: an `Arc<Pcm>` and a cursor. `unload` drops the registry's Arc;
  playing voices keep theirs (today's semantics, for free).
- **Stream**: a ring the decode thread fills to a lookahead (named
  constant) from its `AudioDecoder`; loop is the decoder seeking to 0 and
  filling on, seamless because the ring is continuous; `setLoop(false)`
  lets the current pass end. A decode error logs once (on the decode
  thread) and ends the ring. A starved ring (a stall, later an HTTP source)
  silences that voice only and counts an underrun; the mix is never held.
  One voice per streaming clip stays the documented contract.
- **Sink**: a ring any one thread pushes into (the video worker, or JS).
  `queued_us` is the ring's fill, `position_us` the frames the callback
  consumed (today's "pushed minus queued", exactly), pause holds, clear is
  the producer-side discard above. The callback also publishes
  (consumed frames, Instant) through a seqlock pair so a reader can
  interpolate the position between blocks; wiring that into the video
  transport's `AudioSync` stays video-playback's stage 3.

### Device

One stream on the default playback device, f32 stereo at the device's own
rate from `SDL_GetAudioDeviceFormat` (so the final stage never resamples),
opened with the get-callback, lazily on first use as today, kept for the
process lifetime. `outputSampleRate()` reports that rate. SDL migrates the
stream when the default device changes. The block size is SDL's default
(`SDL_HINT_AUDIO_DEVICE_SAMPLE_FRAMES` left alone); measured per device
into Findings. The output path is the same SDL device stream video uses
now, so `AUDIO_OUTPUT_LATENCY_US` stands and is re-verified, not retuned.
No device (headless box): the open fails and `load`/`play` return the
error, as today.

### Observability

Audio stats through the existing stat registration: live voices, buses,
underruns, the callback's maximum render time per second. The last one is
the number that says whether the mixer keeps its budget on the TV.

## Decoding

- **WAV** (`forge/src/audio/wav.rs`): as decided above. Streaming is
  trivial (seek is an offset into `data`).
- **`.opus`** (`forge/src/audio/opusfile/{ffi.rs, mod.rs}`): `op_open_callbacks`
  over the caller's `Read + Seek` (read, seek, tell callbacks; the `Seek`
  object owned by the handle), `op_read_float`, `op_pcm_total`,
  `op_pcm_seek`, `op_channel_count`; pre-skip, header gain and chaining are
  the library's. Output is 48 kHz by definition. `http.c` is excluded
  (`OP_DISABLE_HTTP`). libogg (`framing.c`, `bitwise.c`) and libopusfile
  (`opusfile.c`, `info.c`, `internal.c`, `stream.c`) as submodules pinned
  to release tags, compiled by `cc` in forge's build.rs against the libopus
  it already builds.
- **WebM/Opus** (`forge/src/audio/webm.rs`): the `WebmDemuxer` with the
  video track off, `OpusDecoder`, the shared trim rule. An audio-only
  `.webm`/`.weba` is what `ffmpeg -c:a libopus` writes.
- Third-party notices: rows for libogg and libopusfile; the SDL_mixer and
  stb_vorbis rows go.

## JS surface

`flux:audio`:

- `setBusGain(bus, gain, { rampMs? })` implemented as declared: audible
  level = voice x bus x master, each layer independent, live and future
  voices, reset on reload.
- `Playback`: `pause()`, `resume()`, `position(): number` (seconds),
  `setLoop(loop: boolean)`.
- `Clip`: `duration: number | undefined`.
- Module: `pause({ bus? })`, `resume({ bus? })` beside `stop`.
- `createSink({ sampleRate, channels })` -> `{ id, push, queuedMs,
  positionMs, pause, resume, clear, setGain, close }` per
  [audio-pcm-sink-js](../backlog/audio-pcm-sink-js.md); `bus?` in its
  options, so a sink is under master and bus like every voice. The `id`
  is what a runtime module passes to `pcm_sink_handle`. One pusher per
  sink is the contract (the ring is single-producer).
- `play`/`load`/`stream` accept WAV, `.opus` and WebM/Opus; the docs say
  so and name what is rejected.

`flux:video`: `open(src, { bus? })` and `player.setGain(gain, { rampMs? })`,
so video audio obeys master and bus gains.

Mirrored in `packages/flux-types`, the core `Sound` (plus `createSink`
beside `createMicrophone`), `docs/50-runtime`, `packages/cli/assets.d.ts`,
`packages/cli/agents/assets.md`, `examples/spin/AGENTS.md` (format wording).

## Deletions

`sdl3-mixer-sys` and the `mixer` feature of `sdl3` in all four target
blocks of `alloy/Cargo.toml`; the `MIX_*` raw setters in `sdl_utils.rs`;
`RampDriver` and the leaked `Mixer`; `alloy/examples/audio_rate_probe.rs`;
the SDL_mixer and stb_vorbis rows in `scripts/build-third-party-notices.ts`;
the "not implemented" `setBusGain` body and its interim-pattern doc.

## Stages

1. DONE 2026-10-15. **`forge::audio`**: `Pcm`, `AudioDecoder`, sniffing `open`/`decode`,
   the WAV parser, the vendored libogg + libopusfile with build.rs and the
   FFI module, the WebM audio decoder with the shared trim rule, the
   feature split, notices rows. Tests in `forge/src/tests/audio.rs`: a WAV
   fixture table (8/16/24/32/f32, mono/stereo, extensible, LIST/JUNK
   chunks, odd-size pad, truncated data, ADPCM rejected by name), an
   `.opus` round trip against its WAV source within tolerance and a seek
   that lands, the existing `video_av.webm` decoded audio-only, Ogg
   Vorbis rejected with the Opus message. Fixtures are small and checked
   in.
2. DONE 2026-10-15 (desktop; devices at stage 4). **The mixer**, one change: the SPSC ring, the engine core (voices,
   buses, ramps, envelope, pan, rate, loop, pause, position; clip, stream
   and sink voices), the decode thread, the SDL device and callback and the
   converter streams, the alloy API over it, SDL_mixer and the ramp thread
   deleted. The core renders through a small `Converter` trait (the SDL
   stream, and an identity converter for tests), so
   `alloy/src/tests/audio.rs` renders deterministic blocks: ramp values at
   sample positions, the pan law, loop seam continuity, fade end and
   retirement, voice x bus x master, bus pause, sink position and discard,
   stale-slot commands ignored. Then `probes/audio-mix-probe.tsx` (15
   checks) and `examples/audio` pass unchanged, and `avsync.webm` is in
   sync on desktop.
3. DONE 2026-10-15. **Surface**: everything under "JS surface", types, core, docs, the
   probe extended for bus gain, pause/resume, position/duration, setLoop,
   the sink and the video bus, plus an app test under `sol test`.
4. DONE 2026-10-15. **Verification and close**: desktop, the tablet and the TV, release
   builds: clip audio (overlap, ramps, fades, buses, pause, loop at
   non-unity rate), streams, a JS sink, video under master/bus with
   `avsync.webm` on the TV's speakers, a reload mid-mix, stats read. Then
   the move to done with Findings cut to notes.

## Findings

Cut to [audio-unification-findings](../notes/audio-unification-findings.md)
at completion. A/V sync on the TV's own speakers confirmed by eye and ear
on 2026-10-15 (avsync.webm via examples/video/src/probe/avsync.tsx).
