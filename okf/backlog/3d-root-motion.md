---
title: In-place clip playback - root motion detected and cancelled by the mixer
description: Authored clips carry root motion, so any fixed-camera viewer sends the character walking off screen and every app rediscovers pinning by hand; give createMixer a per-clip in-place mode that detects travelling clips by NET DRIFT of the root track, rebases the baseline at play, and strips the motion inside the facade.
created: 2026-09-03
---

# In-place clip playback

## Symptom

Locomotion clips translate the root - the motion a game consumes to move
the character - so playing one in a viewer, portrait or loadout screen
sends the model through the lens. Every animation system grows the switch
(Unity `applyRootMotion`, Three's mixer + userland root strip); ours has
nothing, and the app-side version costs each app the same two hard-won
lessons:

- The test for "needs pinning" is the root track's NET DRIFT (last key
  minus first), not how far it moves: taunts and idles wander without
  drifting, and pinning those pushes the slide into the feet - the exact
  artefact pinning exists to remove.
- Clips are not authored against one root baseline (one export's clips
  ride ~1.9 units above its rest pose), so the correction must rebase
  per clip, captured when the clip starts, without flattening the
  vertical bob.

The reference implementation of both (drift classification from baked
channels, per-play baseline capture, x/z pin) lives in a demo app's
`cancelRootMotion` and costs ~0.02 ms/frame of onFrame JS since the core
evaluator landed - cheap, but still per-app rediscovery of nontrivial
rules.

## Shape

Facade-level, zero core changes ([animation-core](../done/animation-core.md)
kept per-channel registration granularity FOR this): `createMixer` (or a
play option) classifies each clip once from its baked root channel -
net drift past a threshold = travelling - and for travelling clips either
registers a variant clip with the root translation channel dropped, or
keeps the app-side pin (one setTransform in the mixer's own frame hook)
against a baseline sampled at play(). Per-clip and automatic by default,
overridable per play; document the drift-not-extent rule where the option
is.

## Done looks like

A fixed-camera app plays every clip of a stock rig - idles, taunts, run
cycles - with no root handling of its own; the demo deletes its
`cancelRootMotion`. Ships together with or after
[3d-skeleton-sharing](3d-skeleton-sharing.md), which owns the other half
of that app's remaining per-frame JS.
