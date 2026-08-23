---
title: Mipmaps
description: "No mipmaps exist" was a documented axiom, so any minified texture aliased. Landed 2026-08-23 as `mipmap` on the creation-time sampler options - id state next to filter/wrap, regenerated after every upload and every target render, minified through by shader sampling - verified on Linux by readback.
created: 2026-07-30
completed: 2026-08-23
---

# Mipmaps

From [gpu-review](../notes/gpu-review.md) (lesson 15). Minification
without mipmaps is aliasing, not a style: the 07-15 maturity assessment
recorded it from the Doom port ("distant surfaces alias"), and the
supersampling path in [gpu-target-antialiasing](gpu-target-antialiasing.md) is capped at 2x for the
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
([gpu-sampler-state](gpu-sampler-state.md) landed the slot). When set: allocate storage with
mips, min filter goes trilinear (LINEAR_MIPMAP_LINEAR via the sampler
objects - a new SamplerCache dimension), `generateMipmap` after upload
(data textures) or after render (targets, off the flush). `nearest` +
`mipmap` is the pixel-art-at-distance combination (NEAREST_MIPMAP_LINEAR),
legal but secondary.

## Landed 2026-08-23

The shape above, with one simplification: no storage change at creation -
`glGenerateMipmap` allocates the chain itself. `SamplerState` gained
`mipmap`, `SamplerCache` went from four sampler objects to eight
(`LINEAR_MIPMAP_LINEAR` for linear, `NEAREST_MIPMAP_LINEAR` for nearest),
`GpuTexture::upload` regenerates when declared, and `ShaderTexture::resolve`
- now the tail of every content write (render, overwrite, clear; it also
runs after fragment renders, which had no MSAA resolve to go through) -
regenerates a target's chain. Plugin, flux-types, core `gpu.ts` (the memo's
rebuild rule includes `mipmap`) and docs/reference/gpu.md updated.

Verified on Linux via the control API (probes/mipmap-probe.tsx): a 256x256
one-pixel checker sampled into an 8x8 target reads 255 everywhere with
nearest and no chain (aliased to one texel), 128 everywhere with
`mipmap: true` on a data texture, and 128 everywhere for a shader TARGET
with `mipmap: true` (the automatic post-render regeneration).

Deliberate limits: the `<texture>` display draw samples level 0 only
(Impeller per-draw sampling), so a supersampled target shown through
`<texture>` still stays at 2x; and `mipmap` is id state, not overridable
per binding - see [gpu-per-binding-sampler](gpu-per-binding-sampler.md),
which landed together with this.
