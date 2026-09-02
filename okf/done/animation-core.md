---
title: Animation core - clip sampling and blending as a producer into the spatial arena
description: There is no animation system; per-frame clip sampling is O(animated nodes) interpreted work and skinning is O(vertices), both below the interpreter line, so character-driven apps are blocked. Build a core evaluator that samples baked keyframe tracks and writes node TRS into the spatial arena each frame, with JS keeping the O(changes) policy (play, stop, crossfade, state machines); skinning follows as bone palettes through the planned TextureSlot sink.
created: 2026-08-24
completed: 2026-09-03
---

# Animation core

## Symptom

Nothing animates a node except app code in `onFrame`. That is fine for a
spinning cube and wrong for anything clip-driven: sampling keyframe tracks
and blending them is per-animated-node-per-frame work, and at the ~10 us
per interpreted step of the signal-path figures a handful of characters
saturates the frame budget before gameplay runs a line. Skinning is worse -
per-vertex, ruled out in JS outright (3d roadmap item 16 already routes
bone matrices through a float texture for this reason). The result is that
roadmap item 7 (glTF) would deliver models that cannot move, and there is
no animation entry on the roadmap at all.

This is the same shape [spatial-core](spatial-core.md) fixed for the scene
walk: O(per-frame) work at rung 1 of the escape ladder
([3d-differentiators](../notes/3d-differentiators.md)). The spatial-core
note already anticipates the two hooks this item consumes: "native
transitions on node transforms, the spatial analogue of the 2D tree's
shipped native transitions" (the smaller sibling of this item,
[spatial-node-transitions](spatial-node-transitions.md)), and the
`TextureSlot` sink row ("skeleton bones for skinning").

## Shape: a producer into the arena, not a sink

Sinks consume the flush's output. Animation is the other side: a
**producer** that writes node-local TRS *before* the flush, through the
same `set_transform` path JS uses today. That keeps the layering clean -
the spatial module needs zero changes for stage 1, the sink admissibility
rule is not even in play, and dirty propagation, sinks, picking refits and
the transparent-order check all just see moved nodes.

Per frame, on the main thread, before `flush()`:

- **Tracks** are baked buffers: per-channel times + values (translation,
  rotation, scale), glTF's three interpolations (step, linear, cubic
  spline). No runtime parsing - the CLI bakes them at pack time under Bun
  from the mature loaders, the exact direction item 7 already committed to
  for meshes. Runtime receives Float32Arrays.
- **Players** hold (clip, time, speed, weight, loop). The evaluator
  advances time by the frame dt, samples each active channel, blends by
  weight (nlerp or slerp for rotation - decide once, document), and writes
  the result to the target node's local TRS.
- **JS owns policy**: play/stop/crossfade, state machines, gameplay-driven
  weights. All O(changes) - a crossfade is two weight writes, not per-frame
  traffic. Completion and loop events come back frame-batched, the same
  pattern as pointer moves.

The 2D precedent is native transitions: core already interpolates UI
properties per frame with JS only setting endpoints. This is that idea
with tracks instead of a single ease.

## Stages

Stage 1 - node tracks: DONE 2026-09-03. `alloy/src/spatial/players.rs`:
a clip registry (packed channels cross FFI once per clip) and players
(clip, target NodeId table, time, speed, weight, fade, loop) that sample
the glTF triple (step/linear/cubic, slerp short-arc, cursor-cached key
lookup), blend per (node, path) with the mixer's incremental weighted
average, and write TRS through the snap path. Players advance on the
stamped frame clock BEFORE the frame's JS (lattice runtime.rs, beside
stamp_clock) - deliberately opposite the node transitions' post-JS slot -
so onFrame is the post-animation hook (Unity's LateUpdate lesson);
`flux:spatial` grew createClip/createPlayer/setPlayer/readTransform and
the "spatialClipEnd" event; the draw path's spatial flush went
unconditional so player poses always publish. `createMixer` kept its
surface minus update() (playback self-advances; core-authoritative pose,
the Unity/Godot model - JS mirrors of animated joints go stale, 3d
`getTransform` reads back). Measured on the clip-player probe (48-joint
rig, baked crossfading clips, release client): jsMs 0.04 at 60 fps with
ZERO frame subscriptions, against ~3.7 ms/frame the JS tier cost one
59-joint character. Usage and traps: `packages/3d/AGENTS.md`.

The 2026-08-31 rung-1 measurement that argued the priority (heroes-v2:
~3.7 ms/frame to sample 177 channels for ONE character) is recorded in
the heroes-v2 feedback file, item 12.

Stage 2 - skinning: DONE 2026-09-02, built FIRST (the heroes-v2
measurement showed the palette walk plus its ordering workarounds
outweighed sampling): the `TextureSlot` sink landed in spatial
(`bind_texture_slot`: per-node `{texture, row, post}`, group-level
optional anchor, one whole-palette upload per texture per flush),
`createModel` binds each joint with `post` = its inverse bind and the
model root as anchor (palettes stay model-local, the skinned uModel
contract untouched), identical skins dedupe to one texture, and
`updateSkins` is deleted - palettes compose at the flush, so pose writes
in any order against `mixer.update` land the same frame. The design
question is settled: the per-binding constant post-multiply, NOT the
"palette node" (a TRS local cannot represent a sheared inverse bind, and
it would double the node count per rig); the admissibility rule in
[spatial-core](spatial-core.md) records the widening. Morph targets ride
the same float-texture machinery and stay out of scope until a model
demands them.

## Placement and rules

Main thread beside `spatial/` (whether inside `alloy/src/spatial/` or as a
sibling `alloy/src/animate/` is a naming call, not architecture - it holds
no GL and issues no RasterCmds itself; the spatial flush does). Rendertree
rules apply: engine-independent, native Rust types, marshalling in a flux
plugin (`flux:animate`, or grown onto `flux:spatial` - decide by whether
players make sense without the arena; they do not, which argues for one
module). `flux-types` parity as always.

## Not in this item

A state-machine or blend-tree DSL (app JS, by design), IK, procedural or
physics-driven animation, cloth, animation of non-spatial properties
(that is the native-transitions lane). Root motion and shared skeletons
found their consumer at closing time and are filed:
[3d-root-motion](../backlog/3d-root-motion.md),
[3d-skeleton-sharing](../backlog/3d-skeleton-sharing.md). The rest
returns as its own item when a consumer exists.
