---
title: Spatial audio - emitter and listener nodes on the spatial core
description: Every voice knob a positional sound needs exists (pan, gain, rate, all ramped) but nothing places a sound in the world, so an engine note or a passing kart is per-voice-per-frame JS - compute the relative position and velocity from the camera and write three setters. Bind a playback to a spatial node and name a listener node, and the core writes pan/gain/rate from the flushed world matrices; the JS pattern stays valid.
created: 2026-08-30
---

# Spatial audio - emitter and listener nodes on the spatial core

## Symptom

`flux:audio` is a stereo mixer: a voice has `pan` (a scalar in -1..1),
`gain` and `rate`, each settable live and ramped
([flux-audio-mix-control](flux-audio-mix-control.md)). There is no
listener, no emitter position, no distance law and no velocity anywhere in
`alloy/src/audio.rs` or the `flux:audio` types. A game with sounds that
live in the world (an engine per kart, a waterfall, a shot passing by)
therefore does, every frame, for every live voice: read the emitter's and
the camera's world position, project the offset onto the camera's right
axis for pan, apply an attenuation curve for gain, take the closing speed
for a doppler rate, and write the three setters. That is O(voices) of
interpreted work per frame - the same shape [spatial-core](spatial-core.md)
removed from the scene walk, and its own note already names "emitter
world positions for spatial audio" as a consumer of the index that nobody
built.

The 2D port that drove the voice-control work
([flux-audio-voice-control](../done/flux-audio-voice-control.md)) got by
with per-voice setters because a 2D game has a handful of voices and its
positions are already in screen space. A 3D game has neither property.

## Shape: a sink, not an audio engine

An emitter is a spatial node binding, the same family as the visibility
switch and the `uLightDir` shared slot: after the flush, for each bound
voice the core takes the emitter's world position (and its velocity, the
delta of that position over the frame clock) against the listener node's
world matrix, and writes pan, gain and rate to the mixer through the
existing setters. JS sends intent, O(changes): bind a playback to a node,
set the listener node, set the attenuation parameters once.

- **Listener.** One per app: `setListener(node)`; the camera node in
  practice. The listener's world matrix gives position and the right/
  forward axes; the rig can be any node, a parent of the camera included.
- **Emitter.** `attach(playback, node, { refDistance, maxDistance,
  rolloff, doppler? })` - the Web Audio `PannerNode` vocabulary (inverse
  distance law by default, the one every engine ships), simplified to
  what an app needs: no cones, no HRTF, no orientation on the emitter.
  Detach on stop or on node removal.
- **Output.** Direction -> pan (projection onto the listener's right
  axis, equal-power as today), distance -> a gain multiplier composed
  UNDER the voice's own gain (so `setGain` on a bound voice keeps meaning
  "how loud is this thing", per the bus-gain layering rule), closing
  speed -> a rate multiplier under the voice's own rate. Written only when
  the value moved past a small threshold, so a still scene costs nothing.
- **Fallback.** SDL3_mixer's `MIX_SetTrack3DPosition` does distance
  attenuation natively (noted in the voice-control item); whether to use
  it or compute in the sink depends on the mixer replacement planned in
  [video-playback](video-playback.md). Keep the JS surface neutral either
  way: node in, three parameters out.

Layering: the node-to-parameter math sits beside `spatial/` in alloy (it
reads world matrices, it holds no GL), the setter writes go through the
audio module's existing thread-safe path (the ramp driver already steps
setters off-thread). Marshalling in the `flux:spatial` or `flux:audio`
plugin - decide by which handle the app holds; the playback argues for
audio. `flux-types` and the core `Sound` wrapper mirror it (`play({ node
})` is the likely reactive spelling).

## Done looks like

A scene with twenty engine voices bound to kart nodes and the listener on
the camera rig: sounds pan, fade and doppler as karts pass with zero
per-frame JS on the audio side, verified by the spatial-core bench pattern
(cost proportional to bound voices, not scene; nothing written while
nothing moved). The hand-rolled pattern above keeps working for a voice
that is not bound.

## Not in this item

Reverb zones, occlusion, HRTF/binaural output, emitter cones. Each is a
later additive option on the same binding.
