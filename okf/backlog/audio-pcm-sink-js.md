---
title: JS cannot stream PCM to the audio output
description: flux:audio plays whole clips only (load, loadPcm, stream), so speech synthesis and live processing have no way to play samples as they are produced; alloy's PCM sink exists for the video plugin, and a createSink in flux:audio over it, with an id a runtime module pushes into from its own thread, closes the loop the microphone ring opened on the input side.
created: 2026-10-11
---

# JS cannot stream PCM to the audio output

## Symptom

`flux:audio` plays whole clips: `load` (Ogg or WAV bytes), `loadPcm`
(samples generated up front, one clip) and `stream` (a file decoded on
demand). Nothing takes samples as they are produced. Speech synthesis one
sentence at a time plays as a series of clips with a seam at each join
unless the app times them itself, and live processing (noise suppression
on what the user hears, a voice changer, a synth) has no way to play its
output at all.

The engine exists: alloy's PCM sink (`create_pcm_sink`, `pcm_sink_push`,
`pcm_sink_queued_us`, `pcm_sink_position_us`, pause, gain, clear, and
`PcmSinkHandle` for pushing from another thread, `alloy/src/audio.rs`).
Its only user is the video plugin; `flux/src/alloy_plugins/audio.rs`
notes that PCM sinks are the video plugin's. A runtime module can already
push from Rust once it has a handle, so the gap is the JS side and the id
that gets a module the handle. Feedback item 7 of the `vision` demo; the
pair of okf/backlog/microphone-tap.md (item 6).

## Shape

- `createSink({ sampleRate, channels })` in `flux:audio`, returning a
  bound sink object `{ id, push, queuedMs, positionMs, pause, resume,
  clear, setGain, close }`, one to one over the alloy calls. `queuedMs` is
  the backpressure signal the alloy doc already names: push while it is
  under a lookahead, wait otherwise.
- The id is what a runtime module passes to `pcm_sink_handle(id)`, so the
  app owns the sink (its lifecycle, its gain) while a module pushes into
  it from its own thread: speech synthesis on an inference thread, the
  same split as the camera tap and the microphone ring. Closing the sink
  ends the module's pushes with an Err, as a closed ring does its readers.
- A reactive wrapper in core beside `createMicrophone`: `createSink`
  owning the sink's lifecycle.
- The relation to the mixer, stated: a sink is an SDL device stream
  outside SDL_mixer, so `setMasterGain`, `setBusGain` and `stop()` do not
  reach it today. Either it becomes a track on a bus or the docs say it
  stands apart; the video audio has the same question and decides with
  it (okf/backlog/video-playback.md names the sink as the mixer's pilot
  replacement).
- A test shape: a sink under `sol test` plays into nothing but reports
  `queuedMs` and `positionMs` deterministically against the frame clock,
  or the test source counterpart is left for its own item.

## Done looks like

- An app pushes Float32Array chunks and hears a seamless stream; a module
  in a custom runtime pushes into a sink the app opened, from its own
  thread, and stops when the app closes it.
- With the microphone ring, a model in a runtime module has a full audio
  loop with no JS in the sample path: a tap in, a sink handle out.
- `flux-types` documents the sink and its mixer relation.
