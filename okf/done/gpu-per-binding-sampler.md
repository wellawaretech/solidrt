---
title: Per-binding sampler override
description: filter/wrap are fused into the texture id, the right default, but it left no escape hatch. Landed 2026-08-23 - a `textures` binding value may be `{ id, filter?, wrap? }`, overriding the texture's declared sampling for that binding only; mipmap stays id state. Verified on Linux by readback.
created: 2026-07-31
completed: 2026-08-23
---

# Per-binding sampler override

From [gpu-review](../notes/gpu-review.md) (lesson 8), filed 2026-07-31
when the review's shortlist closed and this was one of two ranked lessons
with no home.

Both standards deliberately separate the sampler from the texture - WebGL2
added sampler objects *specifically* to undo the WebGL1 fusion, and WebGPU
never had it. Their reason is real: the same texture legitimately wants
different sampling in different passes.

[gpu-sampler-state](gpu-sampler-state.md) (2026-07-29) fused `filter`/`wrap` into the texture id
- the WebGL1 model - while implementing it with the WebGL2 machinery (shared
sampler objects in `SamplerCache`, bound per input unit in `run_pass`). The
fusion is a deliberate solidrt-lens call and it is right for the common case:
one texture, one look, and it makes `<texture>` display and shader sampling
agree by construction, which a separate sampler object cannot.

What is missing is the escape hatch, and the cases are ordinary:

- a nearest-filtered pixel-art atlas cannot be sampled linearly by a blur
  pass;
- a clamp-wrapped target cannot be tiled by one consumer that wants repeat.

## Shape

Widen the `textures` binding value from an id to an id-or-object:

    textures: { uTex: texId }                          // unchanged, texture's own state
    textures: { uTex: { id: texId, filter: "linear" } } // override for this binding only

Cheap engine-side: `SamplerCache` is already keyed by sampler state and bound
per unit, so an override is a different cache key on one unit, not new
machinery. The binding list carries `(name, id, Option<SamplerState>)`
already in the window-shader path's shape.

Two things to keep straight when it lands:

- The fusion's guarantee (display and sampling agree) survives only for the
  default form. An override is a per-pass deviation by construction, so the
  docs should say the texture's own state is what `<texture>` paints and what
  any binding without an override uses.
- The binding marshal is the same site that call-site validation
  ([gpu-callsite-validation](gpu-callsite-validation.md)) checks names against the reflected uniform
  table, and the same site branded ids ([gpu-branded-ids](gpu-branded-ids.md)) type. A widened
  value type touches both; neither changes shape.

## Landed 2026-08-23, with mipmaps

Picked up together with [gpu-mipmaps](gpu-mipmaps.md) because both are the
same mechanism (`SamplerCache` keyed by state, one sampler object per unit)
and the rule that keeps them apart is only obvious when designed at once:
**`mipmap` is id state and not overridable** - generation is a property of
the texture, and a sampler asking for mip levels on a texture without a
chain is sampling-incomplete. An override carries filter and/or wrap only.

The binding tuple `(String, u64)` became `alloy::TextureBinding { name, id,
sampler: SamplerOverride }` everywhere (spec, commands, target state, the
UI-side sampler-graph mirror, inventory, both JS decoders - the `flux:gpu`
calls and the JSX `shader.textures` prop). `merge_bindings` replaces a named
binding whole, so a rebind without an override drops the old one. The
resolver computes `texture.sampler.overridden(&binding.sampler)` and asks
the cache; the graph mirror ignores overrides (they do not change edges).
`/gpu` reports an overridden binding as `{ id, filter?, wrap? }`.

Verified on Linux (probes/mipmap-probe.tsx): a nearest checker sampled
minified reads a single texel (255) through a plain binding and the
bilinear average (128) through `{ id, filter: "linear" }`.
