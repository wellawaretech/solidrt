---
type: backlog-item
title: Compressed texture uploads (ETC2)
description: ES 3.0 mandates ETC2/EAC in core, a free 4-8x texture memory cut on every GL target, but uploadTexture is RGBA8-only; demand-gated, with the honest caveat that ANGLE on Windows may software-expand it - the same split that made both web standards gate the feature.
status: deferred
timestamp: 2026-07-30T00:00:00Z
---

# Compressed texture uploads (ETC2)

From [gpu-review](../analysis/gpu-review.md) (lesson 17). OpenGL ES 3.0
requires ETC2/EAC in core, so every GPU at alloy's minimum spec decompresses
it in the sampler for free - 4-8x less texture memory than RGBA8. Both web
standards expose compressed formats but gate them (WebGL2 behind an
extension, WebGPU behind a feature) for a reason that applies here too:
desktop-emulation backends - ANGLE over D3D, the Windows path - lack the
hardware format and must transcode or decline. `createTexture`/
`uploadTexture` are RGBA8-only today, so a game-scale texture set pays the
full multiple everywhere.

Shape when demanded: a `format` option on createTexture (RGBA8 default,
`etc2-rgba8` first), raw compressed bytes in, `glCompressedTexImage2D`
under it; `Flux.capabilities` (or [[gpu-labels-limits]]'s limits object)
answers whether the device takes it natively. Mutable/resize paths and the
`<texture>` display draw are unaffected (a texture id is a texture id).

Demand-gated: no field report asks, and the workload that would (large
game texture sets) has not appeared. Filed so the platform caveat is
recorded with it: guaranteed native on the GL targets (Linux, Android),
possibly software-expanded under ANGLE on Windows.
