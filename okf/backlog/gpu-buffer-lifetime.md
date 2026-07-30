---
type: backlog-item
title: Buffers held like programs
description: Buffers are the one GPU id space with an ordered-destroy rule ("destroy pipelines drawing from it first") whose violation silently freezes geometry; holding them by Rc from targets, like programs and pipelines, deletes the rule and the failure mode together.
status: done
timestamp: 2026-07-30T00:00:00Z
---

# Buffers held like programs

Shipped 2026-07-30, exactly per the shape below: the raster registry stores
`Rc<GpuBuffer>`, each target's MeshState clones the Rc at create (it also
keeps the registry id for write-driven re-renders), `DestroyBuffer` goes
through the new `gpu::release_buffer` (the `release_program` pattern), and
`ShaderTexture::destroy` releases the target's use. The ordered-destroy
sentence is gone from `destroy_gpu_buffer`, `gpu.d.ts`, and core `gpu.ts`;
`writeBuffer` to a destroyed id still errors (the id retires immediately).

From [gpu-review](../analysis/gpu-review.md) (lesson 9), shortlist item 4 -
the cheapest correctness item on its list.

The API has three lifetime rules where both web standards have one:
textures are reclaimed frame-safely once the tree stops referencing them;
programs and pipelines are held by Rc so either destruction order is safe;
buffers alone are manual and ordered - "Destroy pipelines drawing from it
first" (`destroy_gpu_buffer`, alloy/src/context.rs). Destroy out of order
and the VAO's reference keeps the GL storage alive, so targets keep drawing
stale geometry while writes to the id error: a silent-freeze footgun in an
API whose other id spaces made the same mistake structurally impossible.

## Shape

Hold buffers the way targets already hold their pipeline (and pipelines
their program): the raster registry stores `Rc<GpuBuffer>`, each
target's MeshState clones the Rc at create, `DestroyBuffer` drops the
registry entry, and the GL buffer is deleted when the last user is gone
(`release_program`/`release_pipeline` are the pattern to copy). The
ordered-destroy sentence disappears from the docs
(packages/flux-types/gui/gpu.d.ts, core gpu.ts), and destroy-buffer-first
becomes safe instead of documented-against.

One behavioural note to decide in passing: after the owner drops the
registry entry, `writeBuffer` to that id still errors (the id is retired) -
that part of today's contract stays.
