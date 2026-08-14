---
title: Mipmaps
description: "No mipmaps exist" is a documented axiom, so any minified texture aliases (the Doom port's distant surfaces, 4x supersampled targets); GL gives generateMipmap for free and the dirty flush makes render-target regeneration automatic, so the shape is a mipmap option on the sampler state.
created: 2026-07-30
---

# Mipmaps

From [gpu-review](../notes/gpu-review.md) (lesson 15). Minification
without mipmaps is aliasing, not a style: the 07-15 maturity assessment
recorded it from the Doom port ("distant surfaces alias"), and the
supersampling path in [[gpu-target-antialiasing]] is capped at 2x for the
same reason (4x skips texels on the way down).

Both standards have mipmaps; they split on generation. WebGL2 is one call
(`generateMipmap`); WebGPU deliberately shipped without automatic
generation (apps build mip chains with render passes) and it is one of that
spec's most-complained-about austerities. Sitting on GL, alloy gets
`glGenerateMipmap` for free - take WebGL's convenience.

The retained model adds something neither standard has: the dirty flush
knows exactly when a target re-rendered, so mip regeneration for render
targets can be automatic (regenerate after render, before consumers
sample), where both standards make the app schedule it.

## Shape

`mipmap?: boolean` at creation on SamplerOptions, next to `filter`/`wrap`
([[gpu-sampler-state]] landed the slot). When set: allocate storage with
mips, min filter goes trilinear (LINEAR_MIPMAP_LINEAR via the sampler
objects - a new SamplerCache dimension), `generateMipmap` after upload
(data textures) or after render (targets, off the flush). `nearest` +
`mipmap` is the pixel-art-at-distance combination (NEAREST_MIPMAP_LINEAR),
legal but secondary.

Demand-gated: retro/pixel-art wants no mips, 3D minification wants them; no
field report has asked yet. Filed so the sampler-state design slot is used
rather than rediscovered.
