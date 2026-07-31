---
type: backlog-item
title: GPU object labels and device limits
description: Debug labels on every GPU create (surfaced in get_gpu_resources and error strings) and a queryable gpu.limits with bounds checks at create, so oversize targets fail as "exceeds this device's limit 8192" instead of "framebuffer incomplete 0x8cd6".
status: done
timestamp: 2026-07-30T00:00:00Z
---

Status 2026-07-31: both halves DONE and runtime-verified on five clients
(Linux, Windows/ANGLE, three Android including the 2017 TV). Every device
reported its own ceilings rather than the GLES floor - 16384/32/16,
16384/16/16, 16383/128/32, 16384/16/16, 8192/16/16 - and both bounds checks
threw naming the limit ("16385x16 exceeds this device's max texture size
(16384)"; "33 sampler inputs exceed this device's texture unit limit (32 per
pass)"). Labels appeared on every texture, buffer, program, pipeline and
target in get_gpu_resources, and were inherited by target output textures.
The device reporting maxTextureSize 16383 is the case that justifies querying
rather than assuming. Details in the verification section of
[gpu-review](../analysis/gpu-review.md).
Limits: `GpuLimits` queried at raster startup, cached UI-side via a one-time
Limits RPC, checked at every create/bind/resize site with the limit named,
`run_pass` unit-cap backstop, `limits` exported from flux:gpu and
@solidrt/core; texture creates became fallible (they panicked on adopt
failure before). Labels: `label?: string` on every create (createBuffer and
linkProgram gained an opts bag), stored raster-side (GpuTexture covers plain
textures and targets; GpuBuffer, ShaderProgram, RenderPipeline), surfaced as
`label` in every Gpu*Info and the get_gpu_resources JSON, `7 (bloom-h)`
naming in raster-side messages, labels survive id-stable resizes.

# GPU object labels and device limits

From [gpu-review](../analysis/gpu-review.md) (lessons 4 and 5), shortlist
item 7. Two small features filed together because both are diagnostics for
the same tooling.

## Labels

Every WebGPU object takes a `label` that appears in error messages and
captures; WebGL bolted on KHR_debug late. Here: `label?: string` on every
create (textures, targets, buffers, programs, pipelines), stored in the
registries, surfaced in `GpuTextureInfo`/`GpuBufferInfo`/
`GpuRenderPipelineInfo`/etc. and prepended to raster-side error strings.
Turns "target 7 sampling buffer 3" into "bloom-h sampling particle-verts" -
near-zero cost, pays off the first time anyone debugs a chain of six
targets through get_gpu_resources. Also the natural key for the per-target
pass attribution still open in [[gpu-pass-timing]].

## Limits

Both standards expose the device ceiling because every limit is a hard
per-driver cliff. Nothing in alloy queries any today. Two concrete holes
(gpu-review lesson 5):

- A target larger than the driver max fails as `framebuffer incomplete:
  0x8cd6` instead of naming the limit and the size.
- `run_pass` assigns sampler inputs to texture units by enumeration index
  with no cap; past the fragment unit limit (16 minimum on ES 3.0) the
  extra binds fail and the pass draws with garbage.

Shape: query once at raster-thread startup (MAX_TEXTURE_SIZE,
MAX_TEXTURE_IMAGE_UNITS, MAX_VERTEX_ATTRIBS, MAX_RENDERBUFFER_SIZE, ...),
mirror UI-side, expose as a `limits` object on flux:gpu, and check at the
create/bind sites with the limit named in the thrown message. The unit cap
also wants the run_pass guard regardless of the query surface.
