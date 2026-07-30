---
type: backlog-item
title: Branded GPU id types
description: Every GPU handle is a plain number across five id spaces, so cross-space slips like destroyBuffer(textureId) typecheck and usually hit a valid id in the wrong space; branded types in flux-types close the class with no runtime cost.
status: open
timestamp: 2026-07-30T00:00:00Z
---

# Branded GPU id types

From [gpu-review](../analysis/gpu-review.md) (lesson 3), and item 1 of its
shortlist - the cheapest improvement on the whole surface. Originally raised
in the 2026-07-30 session that first compared the API to WebGPU.

Texture ids (including every shader/pipeline target), buffer ids, shader
stage ids, program ids and render-pipeline ids are all `number`, and
TypeScript cannot tell them apart: `destroyBuffer(textureId)`,
`setDrawCount(bufferId, 3)` and `createShaderTarget(programId, ...)` all
typecheck. Every id space starts at 1 and counts up, so a wrong id is
usually a *valid* id in the wrong space - a mystifying runtime error at
best, an operation on an unrelated live resource at worst. WebGL 1.0 solved
this in 1996-era JS with opaque handle types (`WebGLTexture` vs
`WebGLBuffer`); WebGPU kept the split. It is the one place both standards
are less raw than this API.

## Shape

Branded types in `packages/flux-types/gui/gpu.d.ts`, re-exported through
`packages/core/src/gpu.ts`:

    export type TextureId = number & { readonly __texture: unique symbol }
    export type BufferId = number & { readonly __buffer: unique symbol }
    // ShaderStageId, ProgramId, RenderPipelineId likewise

Purely a `.d.ts` change: no runtime cost, no engine work, no API change.
Existing app code passing raw numbers into destroy/etc. surfaces as type
errors only where it was already unsound. The brands must be exported so
apps can write `let ids: TextureId[]`.

Adjacent sites to sweep for the same currency: `<texture src>` in core
types, camera/capture/svg helpers returning texture ids, and the
`textures:` record values on the create calls.
