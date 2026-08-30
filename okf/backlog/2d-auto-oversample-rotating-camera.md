---
title: The tile layer's auto-oversample thrashes under a rotating camera and has no total ceiling
description: TileLayer picks its oversample from the world view's rotated AABB, which swells up to 1.41x as the camera turns and crosses an integer boundary forever, re-baking every resident chunk on each flip; the window-texel budget is per target so it never binds for chunk-sized targets, the explicit prop is read untracked, and fitOversample is not exported.
created: 2026-08-29
---

# The tile layer's auto-oversample thrashes under a rotating camera and has no total ceiling

What it looks like when you hit it: periodic jank in a tile world with a
rotating camera, with `js` around 0.3 ms and nothing in the app doing
anything. `get_gpu_resources` shows the cumulative `passes` count per chunk
target climbing at rest, and `get_stats` shows `postLayout` spikes of
17-22 ms every second or two. Measured on a two-world demo (design 1280x800,
window fit ~1.77x, `displayScale` 1): 35 re-bakes per chunk, chunk textures
at 1536 square instead of 1024 square (4 MB to 9.4 MB each, x24 resident).

Three things in the same code path, listed by cost.

## 1. The rotated AABB is a signal fed into a step function

[packages/2d/src/components.tsx](../../packages/2d/src/components.tsx)
`TileLayer` picks `n` every layout from `getBoundingBoxViewport(world)`.
That box is the axis-aligned bound of the ROTATED world view, so under an
animated `camera.rotation` its size oscillates by up to sqrt(2). The comment
there says a rotated world's AABB "over-estimates, which only rounds up";
true for a static rotation, false for an animated one: at a 1.77x fit the
scale sweeps 1.77-2.5 and `fitOversample` flips between 2 and 3 forever.
Every flip is `setOversample`, which resizes and re-bakes every resident
chunk ([tiles.ts](../../packages/2d/src/tiles.ts) `setOversample`).

Shape, in order of preference:

- Derive the scale from an UNROTATED basis: the layer's own extent times
  `displayScale()` times the ancestor fit and camera zoom, never the
  post-rotation box. Rotation does not change texels-per-world-pixel, so
  it should not enter the pick at all.
- Hysteresis as a belt: only change `n` when the scale crosses a boundary
  by a margin, so an oscillating input cannot drive a re-bake loop. A
  named constant with its comment.
- A cheaper re-bake would soften this but not fix it; the pick is wrong.

## 2. The texel budget is per target, so a chunked layer has no ceiling

`fitOversample(scale, targetW, targetH, budget)` bounds ONE target by the
window's device pixel count. A chunk is ~512 px, tiny against any window,
so `byBudget` sits around 7 and never binds; the budget only protects the
sprite layer, whose target is viewport sized. Total tile-layer texture
memory is resident chunks x n squared with nothing bounding n but
`maxTextureSize`. The demo above sat at ~260 MB across 152 textures at
n = 2/1; on a 2x display the auto-pick wants 4, 16x the memory of n = 1,
allocated silently.

Shape: a `maxOversample` prop on both components (and the matching option
on the primitives), so an app stays adaptive while bounding the downside.
Today the only lever is an explicit `oversample`, which throws the
adaptivity away. Whether the tile layer's budget should also be the layer
total (resident chunks x chunk texels x n squared against the window) is
worth deciding at the same time; per-target is simply the wrong unit for a
chunked layer.

## 3. The explicit prop is read untracked, and the helper is not exported

`pick` runs as the `onLayout` handler and as the apply function of
`createEffect(() => displayScale(), pick)`, both untracked, and reads
`props.oversample` to decide the opt-out. With a signal-backed prop that is
a reactive read in an untracked scope: `STRICT_READ_UNTRACKED` once per
mount, on correct code. One `untrack(() => props.oversample)` per site
(`SpriteLayer` and `TileLayer`), or hoist the opt-out into the effect's
compute.

The workaround for item 1 (compute `n` from an unrotated leaf and pass it
explicitly) is exactly what triggers item 3, and it needs `fitOversample`,
which [oversample.ts](../../packages/2d/src/oversample.ts) does not export
from the package. AGENTS.md already tells an `output` composer to set the
oversample themselves; without the helper that is ~20 lines of
`onLayout` + `getBoundingBoxViewport` + `displayScale` + ceiling logic
copied into every such app, and the copy is the code most likely to be
subtly wrong. Export it.

Verification: the 2d `tiles` example (rotating camera) with
`get_gpu_resources` showing each chunk's `passes` count flat at rest, and
`postLayout` in `get_stats` free of spikes; a `maxOversample` of 2 on a
simulated 2x display holding `n` at 2 in `/tree`.
