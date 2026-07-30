---
type: backlog-item
title: Document the GPU pixel contract
description: Three facts every pipeline author eventually discovers the hard way - clip-space y points down, targets are premultiplied alpha, values are non-linear RGBA8 - are stated nowhere or only in example comments; declaring them is documentation-only and converts silent per-app discoveries into named contracts.
status: open
timestamp: 2026-07-30T00:00:00Z
---

# Document the GPU pixel contract

From [gpu-review](../analysis/gpu-review.md) (lessons 12 and 14), shortlist
item 2. Documentation only - no engine change - which is why it is near the
top of the do-order.

Three facts to state, and where they live today:

- **Clip-space y points down** (row 0 is the top). The fragment path
  absorbed the flip (vUV is top-left origin, documented); the pipeline path
  leaks it - every vertex author must negate y in `gl_Position` or render
  upside down, and the contract lives in one example comment
  (`gpu-pipeline.tsx`, "Clip y is negated..."). Vulkan ships the same
  y-down clip space and simply declares it; WebGPU's convention cleanup is
  the reason half of WebGL's all-time questions (Y-flip) do not exist
  there. The runtime cannot absorb this one (gl_Position belongs to the
  app's shader), so declare it.
- **Targets are premultiplied alpha.** Implied by the particles example's
  output and by how Impeller composites, stated nowhere. Declaring it also
  decides the factor pair for the deferred alpha-over blend mode
  ([[gpu-pipeline-blend-modes]]).
- **Values are non-linear RGBA8** (no colour-space concept): linear
  filtering and additive blending operate on non-linear values. Not fixable
  cheaply and not worth fixing now - but stating it keeps shaders written
  against the real contract correct if a format vocabulary ever arrives
  ([[gpu-pipeline-extensions]] float formats).

Sites: the pipeline/preamble section of `packages/flux-types/gui/gpu.d.ts`
and `packages/core/src/gpu.ts` doc comments, `docs/core.md`'s GPU section -
next to the existing vUV-origin sentence, which is the half of the
coordinate contract already written down.
