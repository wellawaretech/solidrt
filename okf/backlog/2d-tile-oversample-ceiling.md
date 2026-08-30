---
title: The tile layer's texel budget is per target, so a chunked layer has no total memory ceiling
description: fitOversample bounds one target by the window's device pixel count, and a chunk is tiny against any window, so the budget never binds for TileLayer; total tile-layer texture memory is resident chunks x n squared with nothing bounding n but maxTextureSize, and a 2x display silently multiplies it by 16 over oversample 1.
created: 2026-08-29
---

# The tile layer's texel budget is per target, so a chunked layer has no total memory ceiling

`fitOversample(scale, targetW, targetH, budget)` bounds ONE target by the
window's device pixel count. A chunk is ~512 px, tiny against any window,
so `byBudget` sits around 7 and never binds; the budget only protects the
sprite layer, whose target is viewport sized. Total tile-layer texture
memory is resident chunks x n squared with nothing bounding n but
`maxTextureSize`. A measured two-world demo sat at ~260 MB across 152
textures at n = 2/1; on a 2x display the auto-pick wants 4, 16x the memory
of n = 1, allocated silently.

Shape: a `maxOversample` prop on both components (and the matching option
on the primitives), so an app stays adaptive while bounding the downside.
Today the only lever is an explicit `oversample`, which throws the
adaptivity away. Whether the tile layer's budget should also be the layer
total (resident chunks x chunk texels x n squared against the window) is
worth deciding at the same time; per-target is simply the wrong unit for a
chunked layer. Unity caps the same adaptivity with a clamp on an explicit
render-scale prop, so the shape is conventional.

Fixed already (this file used to carry them): the auto-pick thrash under a
rotating camera (rotation's AABB swell is divided back out of the measured
box, whose basis `flexShrink 0` pins to the world's true size, plus a
shrink-margin hysteresis), the untracked `props.oversample` reads, and the
`fitOversample` export.
