---
title: Root motion - in-place playback, and the travel moved onto the character
description: Authored clips carry root motion; the mixer now strips it for viewers (inPlace, by net drift) and, for games, moves it onto the model through a core binding (rootMotion "apply" | "report") - translation and yaw, continuous across loop wraps, verified on Mixamo's standing turns.
created: 2026-09-03
---

# Root motion

## Shipped 2026-09-06: the translation strip

`mixer.play(name, { inPlace? })`, automatic by net x/z drift of the root
position track past `ROOT_DRIFT_RATIO` of the model height, the root being
the topmost node any position channel targets; `createMixer(model, {
rootHeight })` names the rebase baseline. The strip is a second core clip
with x/z held at the first key (cubic tangents zeroed) and y shifted, so it
costs nothing per frame and blends like any clip. Verified on
`probes/root-motion-probe.tsx` (linear run, wandering taunt, cubic hop).
`packages/3d/AGENTS.md` documents it. The two halves below are what is
left; the original shaping follows.

## Shipped 2026-09-06: translation delivery

`createMixer(model, { rootMotion: "apply" | "report" })`: every clip plays
fully pinned (all three root axes held) and the core's new root-motion
binding (`spatial.bindRootMotion(player, clip, channel, anchor?)`,
`alloy/src/spatial/players.rs`) samples the AUTHORED root track at the
player's time, takes the per-advance delta (continuous across a loop
wrap, weighted like the pose) and either adds it to the anchor node
rotated by the anchor's rotation ("apply", zero JS) or reports it as a
"spatialRootMotion" event that `mixer.rootDelta()` accumulates
("report"). Verified on `probes/root-motion-probe.tsx`: apply with a wrap
and a yawed model, cubic keys, report take-semantics. On the way: nodes
the core poses are flagged `_native` so `setTransform` never
short-circuits against their stale JS mirror (a teleport back to the
last JS-written spot used to be silently dropped - for joints too).

## Shipped 2026-09-06: the rotation half

The core binding takes the root's rotation channel too: the yaw (turn
about +y of the root's parent space) is sampled per advance, its step
folded into a half turn and continuous across a wrap (net yaw unwrapped
key by key), reported beside the translation (`{ x, y, z, yaw }`) and,
with an anchor, applied as a turn about the anchor's own up. The variant
holds the root's yaw key by key (each key pre-multiplied by the yaw that
undoes its own turn), so the lean and pitch of a turn stay in the pose.
Translation deltas are un-turned by the yaw delivered so far, so a clip
whose hips wander out and back during a turn ends where it says.
Verified on `external/mixamo-turn/standing_turn_right_90.glb` loaded as a
meshless model in `probes/root-motion-probe.tsx` (its data exported to
`probes/mixamo-turn-right.json`): the model ends at -90.00 degrees and
within 0.2 cm of the clip's net drift, the hips held at their first key.

## Shipped 2026-09-06, same day: the loose ends

Yaw is the swing-twist about the up axis (exact under any lean - the
held root reads zero yaw to ten decimals with a 0.3 rad tilt), the up
axis and a vertical mode are options (`rootMotion: { mode, up?,
vertical? }`; `vertical: "pose"` keeps the height in the pose and
delivers only the horizontal travel), cubic rotation tracks are
linearized at 60 keys/s before their yaw is held, the binding primes at
the player's time so a play() loses no frame, and translation is
un-turned by the clip's own yaw (pre-step, so it cancels the anchor's
pre-step rotation exactly; the mid-step form drifted about one percent
of the travel sideways over a 90 degree turn). Two core unit tests in
`alloy/src/tests/spatial_players.rs` pin the wrap continuity, the turn
and the vertical option without a client.

## Findings

- A `setTransform` equal to the node's stale JS mirror was silently
  skipped on any node the core poses (joints under a player, now the
  model under root motion). Nodes the core writes are flagged `_native`
  and writes to them never short-circuit. Lives in node.ts.
- The key-layout helper of the variant path must be path-aware:
  rotations are four floats per key. Walking a rotation channel with a
  position stride scrambles quaternions into non-unit garbage.
- Root-motion translation must be taken in the root's own facing: rotate
  each step by the INVERSE of the clip's own yaw so far (unweighted,
  pre-step) before applying it in the anchor's frame. Rotating raw steps
  by the anchor's current rotation displaced the Mixamo turn by 24 cm;
  un-turning by the mid-step yaw still drifted one percent sideways.
- The control API's batched `/clock?step=N` does not land exactly N
  frame-lengths of animation time (readings of 18.7 and 20.6 frames for
  N = 20); single steps do. Compare per-step deltas, or pose against
  delivered within one run, never a batch against arithmetic.
- Dota (heroes-v2) rigs never turn through clips (every net yaw under a
  degree; Lina's run is authored in place with zero drift) - a game
  cannot assume clips carry travel, so a controller path that treats
  `rootDelta()` as one optional input is the one that works everywhere.

## Original shaping

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
