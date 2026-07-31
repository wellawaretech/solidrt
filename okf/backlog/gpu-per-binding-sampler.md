---
type: backlog-item
title: Per-binding sampler override
description: filter/wrap are fused into the texture id, which is the right default and makes display and shader sampling agree by construction, but it leaves no escape hatch - a nearest pixel-art atlas cannot be blurred linearly and a clamped target cannot be tiled by one consumer; a per-binding override costs little because the sampler cache is already keyed by state.
status: deferred
timestamp: 2026-07-31T00:00:00Z
---

# Per-binding sampler override

From [gpu-review](../analysis/gpu-review.md) (lesson 8), filed 2026-07-31
when the review's shortlist closed and this was one of two ranked lessons
with no home.

Both standards deliberately separate the sampler from the texture - WebGL2
added sampler objects *specifically* to undo the WebGL1 fusion, and WebGPU
never had it. Their reason is real: the same texture legitimately wants
different sampling in different passes.

[[gpu-sampler-state]] (2026-07-29) fused `filter`/`wrap` into the texture id
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
  ([[gpu-callsite-validation]]) checks names against the reflected uniform
  table, and the same site branded ids ([[gpu-branded-ids]]) type. A widened
  value type touches both; neither changes shape.

Demand-gated: no field report has asked, and the workaround (create the same
image twice under two ids) is obvious if wasteful. Filed so the sampler
design slot is used rather than rediscovered.
