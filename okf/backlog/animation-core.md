---
title: Animation core - clip sampling and blending as a producer into the spatial arena
description: There is no animation system; per-frame clip sampling is O(animated nodes) interpreted work and skinning is O(vertices), both below the interpreter line, so character-driven apps are blocked. Build a core evaluator that samples baked keyframe tracks and writes node TRS into the spatial arena each frame, with JS keeping the O(changes) policy (play, stop, crossfade, state machines); skinning follows as bone palettes through the planned TextureSlot sink.
created: 2026-08-24
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
shipped native transitions" (the smaller sibling of this item), and the
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

Stage 1 - node tracks. Clip registry (create/destroy from baked buffers),
players targeting spatial nodes, sample + blend + write, dt from the frame
clock, finished/looped events. Done looks like: a baked multi-channel clip
drives a node hierarchy with zero per-frame JS, verified by the bench
pattern spatial-core used (cost proportional to animated nodes, not scene).

Stage 2 - skinning. Joint hierarchies are ordinary spatial nodes animated
by stage 1; the palette (jointWorld * inverseBind per joint) lands in a
float texture the vertex shader samples. Needs the float-texture engine
item (roadmap 16) and the `TextureSlot` sink. Open design question, settle
before building: whether a per-joint "palette node" (a child whose static
local transform is the inverse bind) keeps the sink rule pure - its world
matrix IS the palette entry - or whether inverse binds with shear force a
per-binding constant post-multiply on the sink, which is still
transform-shaped but widens the sink contract. Morph targets ride the same
float-texture machinery and stay out of scope until a model demands them.

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
physics-driven animation, cloth, root motion, animation of non-spatial
properties (that is the native-transitions lane). Each returns as its own
item when a consumer exists.
