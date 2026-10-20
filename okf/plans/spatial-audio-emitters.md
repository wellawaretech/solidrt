---
title: Spatial audio - emitter and listener nodes on the spatial core
description: A playback bound to a spatial node (or a fixed position) and heard by a listener (a node, or a target's view - the camera), the core writing its pan, distance gain, doppler rate and occlusion low-pass from the flushed world matrices with zero per-frame JS; the Web Audio PannerNode vocabulary, a per-voice low-pass in the mixer, occlusion over the same occluders culling uses, one law for 3d scenes and 2d layers.
created: 2026-08-30
---

# Spatial audio - emitter and listener nodes on the spatial core

## Problem

`flux:audio` is a stereo mixer: a voice has `pan`, `gain` and `rate`,
each settable live and ramped. Nothing places a sound in the world. A
game with sounds that live in it (an engine per kart, a waterfall, a shot
passing by) does, every frame, for every live voice: read the emitter's
and the camera's world position, project the offset onto the camera's
right axis for pan, apply an attenuation curve for gain, take the closing
speed for a doppler rate, and write the three setters. That is O(voices)
of interpreted work per frame, the shape [spatial-core](../done/spatial-core.md)
removed from the scene walk, and its note already named "emitter world
positions for spatial audio" as a consumer nobody had built. Occlusion
culling ([3d-occlusion-culling](../done/3d-occlusion-culling.md)) then
reserved the other half: "the occluder set is the audio blocker set", a
listener-to-emitter ray over the same node-space triangles.

Two of the item's original assumptions had moved by the time it was
picked up (2026-10-20): the mixer is ours (SDL3_mixer's 3D positioning is
no fallback), and the 3d camera is not a node (it writes a view matrix
per target), so a listener needs a view source beside a node source.

## Done looks like

- **Listeners** (`flux:audio createListener({ node } | { view }, {
  speedOfSound? })`): what emitters are heard by, a node (its local -z
  forward, +x right; Three's AudioListener) or a target's view as
  `setView` last wrote it (the camera itself, a 2d view key included).
  Several may exist, one per scene, so split-screen works: Three's
  `PositionalAudio(listener)`, Godot's listener per viewport.
- **Emitters** on a playback (`playback.attach({ node | position,
  listener, distanceModel?, refDistance?, maxDistance?, rolloff?,
  doppler? })`, `detach()`, the `emitter` play option so a one-shot is
  placed from its first sample): the Web Audio PannerNode vocabulary plus
  a doppler factor; `position` is Unity's PlayClipAtPoint with no node
  to create and destroy around a footstep.
- **The pass** (`alloy/src/spatial/emitters.rs`), at the end of every
  flush: for each emitter whose node, listener or an occluder moved (or
  whose derived velocity is still decaying) it evaluates pan, gain, rate
  and cutoff and hands the writer what changed past an epsilon. A still
  scene writes nothing; twenty karts write twenty placements a frame.
  `/stats audioSpatialWrites` is the figure.
- **The mixer** (`alloy/src/audio/engine.rs`): `spatial_gain` and
  `spatial_rate` ramps UNDER the voice's own gain and rate (setGain on a
  placed voice keeps meaning "how loud is this thing"), the pan through
  the existing ramp, and a per-voice RBJ low-pass (Butterworth Q,
  block-stepped cutoff, bypassed exactly once a ramp lands open).
- **Occlusion**: the shown occluders carrying `audio` (default: gain
  0.25, cutoff 2000 Hz; `false` for glass; `{ gain, cutoff }` for a
  curtain) crossing the straight listener-to-emitter segment compose
  (gains multiply, the lowest cutoff wins), written with a 150 ms glide
  so a wall never clicks. One authoring for sight and sound; a hidden
  occluder blocks neither.
- **Packages**: `@solidrt/core/audio` gains `emitter`/`setEmitter` on
  every Sound and a reactive `createListener`; `@solidrt/3d` gains
  `attachSound(node, sound, options)` over the scene's listener
  (`scene.listener()`, the camera by default, `scene.setListener(node)`,
  `speedOfSound` on createScene) with the enter/leave lifecycle of spring
  bones, and occluder specs carry `audio`; `@solidrt/2d` gains
  `view.listener()` over the view's viewer key.
- Tests: `alloy/src/tests/spatial_emitters.rs` (the laws against closed
  forms, the writes-only-on-change rule, the velocity tail and the
  teleport rule, node and view listeners, occlusion composition and
  release, every release path, validation), `alloy/src/tests/audio.rs`
  (the spatial layers under the voice's own, the filter), the app tests
  `packages/core/tests/audio.test.tsx` and `packages/3d/tests/sound.test.tsx`,
  and `probes/spatial-audio-probe.tsx` (twenty karts circling a wall).

## Decisions

**Listener objects with a node or view source**, not one global
`setListener(node)`. The camera is not a node; a plain node listener
would force every app to mirror the camera into a node per frame, the
JS the item removes. A view source reads the core's camera frame from
the world view `setView` stores (f64, no origin to undo); a node source
reads the node's world matrix. One listener per scene falls out, and
`scene.setListener(node)` re-points it for a third-person rig.

**The lateral angle for pan**, not Web Audio's horizontal azimuth: pan
= (2 / pi) asin(dot(d, right) / |d|). The azimuth hard-pans a source
overhead and every sprite on a top-down 2d map; the lateral angle is the
interaural cue itself and sends both to center, one law for both
packages. Behind the listener folds the same. Inside `refDistance` the
pan blends to center (Unity's spread, Godot's panning_strength, folded
into the one distance every emitter already names), so a kart passing
through you does not flip left to right.

**Web Audio's three distance laws as defined**, distance clamped to
[ref, max] for all three (so past maxDistance the inverse law holds its
last value, as the spec says), the result a multiplier under the voice's
own gain.

**Velocities derived from positions**, as Unity derives them: the delta
over the app-time frame delta, smoothed with an 80 ms time constant
(frame-time jitter would warble the pitch), snapped to exactly zero
under a floor so a stopped emitter's tail ends in a few writes. A
frame's displacement faster than sound is a teleport: the velocity
resets instead of chirping. The doppler ratio is Web Audio's pre-removal
formula with each product capped below c and the ratio clamped to an
octave either way. Doppler is ON by default (Unity's dopplerLevel 1),
`doppler: 0` off. An app-supplied velocity for physics-driven emitters
stays additive.

**A low-pass in the mixer, occlusion over the occluders.** Occlusion by
gain alone sounds like distance, not a wall; FMOD, Wwise and Godot pair
a level with a low-pass. The RBJ biquad costs five multiply-adds per
sample per channel on filtered voices only (bypassed exactly when open).
The segment test reuses the occluder's node-space triangles under its
inverse world (box reject first), re-run only when the emitter, the
listener or an occluder moved; a state change glides over 150 ms rather
than the frame interval, and an edge the ray flickers across hovers
instead of stuttering.

**Ramps are the host's.** The core hands values and a ramp class
(frame or occlusion); alloy's Context glides frame writes over the wall
interval since the previous flush (clamped 4 to 100 ms), so pan, gain
and rate trace a continuous line between frames instead of stepping.

**Lifecycle rules.** `setPan` on an attached playback throws (the
emitter owns the pan); `detach` ramps the gain and rate layers back to 1
and opens the filter, the pan stays. A voice that ends frees its binding
before the next flush (the registry's word); a destroyed node frees its
emitters (voices keep their last values); a destroyed listener frees
the emitters heard by it. In @solidrt/3d a node LEAVING its scene stops
its positional voices (Unity stops an AudioSource with its object; a
looping engine must not outlive its kart). A stereo clip is balanced,
not placed: use mono clips.

**Placed from the first sample.** `bind_emitter` evaluates synchronously
against the tree as it stands (pending writes included) and the plugin
sends the placement in the same mix block as the start, so a one-shot
at a position never plays a plain first block.

**Not here, additive on the same binding**: cones, HRTF, reverb zones,
air absorption by distance (a distance-driven cutoff on the same filter;
Godot ships it, Three, Unity and FMOD do not, and its model would be
ours to invent), virtual voices past maxDistance, an app-supplied
velocity, PCM sinks as emitters (a video at a TV in the world: the sink
ids live in a registry of their own).

## Work

1. DONE 2026-10-21. The mixer: spatial ramps, the low-pass, `SetSpatial`
   and `ClearSpatial`, `set_audio_spatial`/`clear_audio_spatial`, the pan
   guard, `spatial_writes`; tests on deterministic blocks.
2. DONE 2026-10-21. The core: listeners, emitters, view frames, the
   pass, occluder `audio` and the segment test, `SinkWriter::write_emitter`,
   release on voice end, node death and listener death; tests.
3. DONE 2026-10-21. The host glue (ramps, the registry's word before the
   flush), `flux:audio` (`createListener`, `attach`/`detach`, the
   `emitter` play option, teardown), `setOccluder`'s `audio`, flux-types,
   `/stats audioSpatialWrites`.
4. DONE 2026-10-21. `@solidrt/core/audio` (`emitter`, `setEmitter`,
   `createListener`), `@solidrt/3d` (`attachSound`, the scene listener,
   `setListener`, `speedOfSound`, occluder specs' `audio`), `@solidrt/2d`
   (`view.listener()`), AGENTS.md and README.
5. Verification: the app tests under `sol test`, the probe on the
   desktop (the writes figure moving and parked, the wall heard, the
   doppler by ear) and on the tablet; then the move to done with the
   findings cut to a note.

## Findings

Desktop, release client, `probes/spatial-audio-probe.tsx` (twenty karts
circling a wall occluder at 60 fps, 2026-10-21):

- The cost model holds: `audioSpatialWrites` climbs by 19.8 per frame
  while the karts drive (20 placed voices), by exactly 0 over 300 parked
  frames (the velocity tails end within the first second), by 20 for a
  teleport while parked (positions moved, no doppler chirp), and by about
  2 per frame with the listener on kart 0 while the ring drives in
  formation (relative geometry constant; the trickle is the wall's
  occlusion flipping as karts pass behind it from kart 0's point of view).
- The mixer's callback maximum (`audioRenderMaxMs`, per 1024-frame
  block at 48 kHz, 21 ms budget): 4 to 6 ms with the twenty voices
  placed and driving, 0.7 to 1.4 ms with the same voices detached, 5 to
  6 ms placed and parked, doppler on or off alike. No underruns. The gap
  is a mixer follow-up (audio-unification-followups.md): it is not the
  doppler ratio, and the user ruled doppler would have gone to a backlog
  item had it been.
- Every reading past 90 ms (and the 30 fps that came with them) came
  while the laptop's EC clamped every core to 400 MHz (97 to 101 C
  package after the builds; see the thermal clamp note): the detached
  voices then took 5 to 14 ms too. Nothing was measured on a cool machine
  or a device yet.
- App tests: core audio (6) and 3d sound (3) pass under `sol test`; the
  alloy suite passes whole (899). Owed: the tablet run and the TV's
  block budget with placed voices.
