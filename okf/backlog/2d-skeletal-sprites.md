---
title: 2d skeletal sprites - skinned deformation over the spatial palette sink
description: The 2d package has flipbook animation only; Spine/DragonBones-class skeletal characters (bone hierarchies deforming a textured mesh) have no path, even though the core machinery - arena bones, the TextureSlot palette sink, the coming native clip evaluator - already exists or is planned with zero 2d-specific core work.
created: 2026-09-02
---

# 2d skeletal sprites

## Symptom

2d character animation tops out at flipbooks: `packages/2d/src/animation.ts`
steps atlas frames through `setSprite`, which covers pixel-art walk cycles
and nothing above them. The standard tier above - a bone hierarchy
deforming one textured mesh (Spine, DragonBones, Unity's 2D Animation,
Godot's Skeleton2D), the format most 2d character art above pixel-art
ships in - has no path. An app wanting a rigged 2d character today would
hand-roll per-vertex JS, which the interpreter rules out (the same wall
3d skinning hit).

## Why it is cheap now

The 3d skinning work of 2026-09-02 put everything perf-critical into the
shared layer, deliberately bone-free:

- Bones are ordinary spatial-arena nodes; 2d already lives in the arena
  (`sprite.node`, groups, native transitions).
- The `TextureSlot` sink composes `inverse(anchorWorld) * world * post`
  rows into an rgba32f palette texture at the flush - `post` = a 2d
  inverse bind as a mat4, anchor = the skeleton root. Zero core changes;
  this item is the sink's anticipated second consumer (the admissibility
  test in [spatial-core](spatial-core.md)).
- Clip playback: the [animation-core](../done/animation-core.md)
  evaluator (DONE 2026-09-03) targets spatial nodes generically -
  `flux:spatial` createClip/createPlayer drive baked 2d bone clips
  exactly as 3d rigs, zero per-frame JS.

What is genuinely 2d-package work: a skinned mesh primitive in the layer
vocabulary (vertices with joints/weights against the sprite shaders - the
one new pipeline), a rig asset path (likely a bake tool from a Spine/
DragonBones export to buffers + clips, the `srt tool 3d/model` pattern),
and where it sits relative to SpriteLayer draw order. A compact 3x2-row
palette projection on the sink is an OPTIONAL later add (additive, the
`InstanceProjection` pattern); full mat4 rows work from day one.

## Done looks like

A baked rig renders in a 2d layer, bones posed by clip playback and by
direct node writes (both land the same frame, the flush contract), palette
cost native, sort order sane against plain sprites. Demand-gated: shape it
further only when an app brings a rigged 2d character.
