---
title: Sampleable depth
description: A pipeline target's depth is a private renderbuffer, unsampleable by construction, so shadow maps, depth-of-field and SSAO have no path; ES 3.0 has depth textures and sampler2DShadow in core. The storage swap is small; the open question is naming - a target's id names its color, so depth needs an id of its own. Split from gpu-pipeline-extensions 2026-08-11.
created: 2026-08-11
---

# Sampleable depth

Symptom: nothing can sample a depth buffer. A pipeline target's depth is a
private renderbuffer, unsampleable by construction, so the standard
depth-driven effects - shadow maps first, then depth-of-field and SSAO -
have no path. Both WebGPU and GLES make depth a texture; ES 3.0 has depth
textures and `sampler2DShadow` comparison sampling in core
(../notes/gpu-review.md lesson 16 is the origin of this item).

The storage swap (renderbuffer to depth texture) is small. The open design
question is currency: a target's id names its color output, so its depth
needs a name of its own to appear in another target's `textures` list.
Once it has an id, the dependency graph tracks the edge like any other
sampler binding (source re-renders propagate, cycles throw).

First consumer: shadow maps, named as roadmap item 15 in
[3d-roadmap](../notes/3d-roadmap.md) together with a depth-func option
([gpu-depth-func](gpu-depth-func.md)); the map itself binds through the
shared target-level sampler channel (landed 2026-08-06).

The shape is settled in [3d-shadow-maps](../plans/3d-shadow-maps.md), stage 1:
`depth: "texture"` on `createDrawTarget` (the renderbuffer form stays
`depth: true`), `depthTexture(target)` returning the depth's own
sampler-only id (nearest/clamp, dies with the target, aliases to its
owner in the dependency graph), rejected with `samples >= 2`. Comparison
sampling is a later, additive sampler state.

History: deferred bullet of [gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md)
since 2026-07-15; split out 2026-08-11.
