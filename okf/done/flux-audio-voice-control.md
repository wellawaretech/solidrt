---
title: flux:audio live voice control (pan, gain, ended, raw PCM)
description: A playing SoundHandle is stop-only - no pan anywhere, gain fixed at play() time, no finished signal, encoded input only - so a 2D game port cannot express positional audio; per-voice setGain/setPan, an ended signal and a raw-PCM load close it.
created: 2026-08-02
completed: 2026-08-03
---

# flux:audio live voice control (pan, gain, ended, raw PCM)

Source: the wasm game-port demo feedback (2026-08-02), the one place the
port is audibly not the original. The ported engine's sound interface
updates volume (0..127) and stereo separation (0..255) for every live
voice, every tick, as the player moves. flux:audio's whole surface is
`play(bytes, {loop, gain}) -> {stop()}`: neither can be applied to a voice
that is already running, and there is no pan at all. The port stubs the
update and positions sounds only at the instant they start - walk past a
firing enemy and it neither pans nor fades.

Three asks, in order of value:

1. Pan and post-start gain on SoundHandle - `handle.setGain(g)` and
   `handle.setPan(p)` close this completely. SDL3_mixer already ships
   exactly this shape (MIX_SetTrackStereo takes left/right gains,
   MIX_SetTrack3DPosition does distance attenuation); if flux's mixer is or
   could be SDL3_mixer this may be mostly plumbing. Per-voice gain + pan
   alone serves every 2D game.
2. A "still playing" signal. The engine polls whether a channel is still
   audible to reclaim its fixed voice pool; with no finished signal the
   port reads durations back out of the WAV headers it just built and
   retires voices on a timer. An `ended` flag or an onEnded callback
   removes that whole class of bookkeeping. (onended was already noted as
   deferred when the module shipped; this is its backlog home.)
3. Raw PCM input. play/load take encoded Ogg/WAV only, so raw 8-bit mono
   PCM gets a synthesized 44-byte WAV header purely to hand it back. A
   `loadPcm(bytes, {sampleRate, channels, format})` deletes that step for
   every emulator, tracker, synth and retro port.

What already worked: 8-bit-source 11025 Hz audio decoded first time with no
fuss, and load() returning a replayable clip maps cleanly onto "cache the
sfx, start many voices".

Done 2026-08-03. Surface shipped (types renamed SoundHandle/LoadedSound ->
Playback/Clip, no backwards compat kept):

1. Playback gains setGain(g) (>= 0) and setPan(p) (-1..1 clamped, equal-power
   law, the Web Audio StereoPannerNode math; unpanned mono is ~3 dB louder
   than pan 0). pan also joined the play options ({loop, gain, pan}) so a
   voice can start positioned. Pan law + track control live in alloy
   (pan_gains, set_audio_gain/set_audio_pan); plugin marshals only.
2. ended(): poll method on Playback (HTMLMediaElement vocabulary; no
   onEnded callback, no audio-thread pump - a missing/swept track reads as
   ended). @solidrt/core Sound prunes dead voices with it and gained
   setGain/setPan applying to live + future voices.
3. loadPcm(data, sampleRate, { channels? }) -> Clip. The typed array is the
   format: Uint8Array = u8, Int16Array = s16, Float32Array = f32,
   interleaved when channels: 2 (defaults 1). MIX_LoadRawAudio underneath;
   no synthesized WAV headers needed.

examples/audio/ exercises all three (synthesized clips only, no assets).
