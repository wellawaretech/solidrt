---
title: maxOversample bounds the auto-picked oversample; the texel budget stays per target
description: SpriteLayer and TileLayer take maxOversample (integer >= 1, auto-pick only, explicit oversample already opts out), the cap that bounds tile-world texture memory (resident chunks x n squared) on high-scale displays without giving up adaptivity; the window texel budget stays per target by design - a layer-total budget would shrink quality as chunks allocate.
created: 2026-08-29
---

# maxOversample bounds the auto-picked oversample; the texel budget stays per target

`fitOversample`'s window-texel budget bounds ONE target, which never binds
for chunk-sized tile targets (`byBudget` sits around 7 against any
window), so a tile world's total texture memory - resident chunks x n
squared - had nothing bounding n but `maxTextureSize`: on a 2x display the
auto-pick wants 4, 16x the memory of n = 1, allocated silently (~260 MB
measured at n = 2/1 on a two-world demo).

Resolution, two parts:

- `maxOversample?: number` on `<SpriteLayer>` and `<TileLayer>` (integer
  >= 1, throws on anything else per the dev validation policy). It caps
  the auto-pick only; an explicit `oversample` already opts out of the
  pick entirely and ignores it. The cap overrides the shrink hysteresis -
  lowering it below the current factor is an explicit ask, not
  measurement noise - and a change re-picks through the same effect that
  watches `displayScale`. Not on the primitives: they never pick, and
  clamping an explicit `setOversample` would be validation theater; an
  `output` composer caps its own `fitOversample` result. Adaptive
  resolution elsewhere (Unity's dynamic resolution) bounds itself the
  same way, with an explicit clamp on the scale.
- The budget question is CLOSED as per-target. The window-texel argument
  only ever bounds what is visible, and visible chunk texels at
  oversample n are exactly n squared x window - never waste. A
  layer-total budget would make n shrink as chunks allocate lazily, so
  writing more tiles would silently degrade crispness everywhere:
  quality dependent on content volume is worse than a documented cap.
  The remaining total-memory legibility gap (a per-layer total in
  `get_gpu_resources`) is noted with chunk eviction in
  [2d-baked-layers](../backlog/2d-baked-layers.md) B2, where residency
  is actually managed.

Verified on the tiles example: the auto-pick holds chunks at 960 square
(n = 2 at display scale 4/3 x zoom 0.9); `maxOversample={1}` re-bakes
them to 480 square, and removing it grows them back - both through
reload-on-save against the live layer.

This file also carried the rotating-camera auto-pick thrash and the
untracked prop reads; those were fixed separately (rotation's AABB swell
divided back out of a `flexShrink 0`-pinned basis, shrink-margin
hysteresis, `untrack` around the picks, `fitOversample` exported).
