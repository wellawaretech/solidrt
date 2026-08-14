---
title: GPU draw targets (multi-pass into one target)
description: The multi-pass bullet of gpu-pipeline-extensions, built as a retained ordered draw list - createDrawTarget + addDraw/removeDraw with stable DrawIds, per-entry setters, and the ordering verbs (before on addDraw, setDrawOrder); stages 1+2 implemented 2026-08-04.
created: 2026-08-04
completed: 2026-08-04
---

# GPU draw targets

Resolves the "multiple draw passes into one target" bullet of
[gpu-pipeline-extensions](gpu-pipeline-extensions.md), unblocked twice over
(the object-model split gave draw state a home on RenderPipeline; the purity
decision plus the correction in okf/notes/scene-graph-3d.md gave it a
legal shape) and driven by the scene-graph workload recorded there: a scene
frame is N draws - one per mesh/material - sharing one depth buffer, with
per-draw uniforms.

## The model

A **draw target** is a render target whose contents are an ordered, MUTABLE
list of draw entries, rendered as one pass: clear once (color unless loadOp
"load"; depth always, when declared), then every entry in list order into
the same storage. WebGPU's render pass (`beginRenderPass` + N x setPipeline/
setVertexBuffer/setBindGroup/draw) turned from a per-frame event into a
retained object - the same move the single-draw target already made, with
render bundles as the standards-world precedent for wanting the retained
form.

Key decisions, in the order they were argued (2026-08-04 session):

- **Retained list, legal on `render: "auto"` targets.** The list is input
  data like params: "render twice = render once" holds, so the dirty flush
  is untouched and a static scene costs zero passes. Only cross-render
  accumulation is non-pure (unchanged: manual + loadOp). One render = ONE
  pass regardless of entry count - the pass-count win on the TV class.
- **Stable `DrawId`s, never indices** (add/remove was designed in from the
  start): target-scoped, UI-allocated, monotonically increasing, never
  reused - a removed entry's id errors instead of aliasing. A scene library
  tracks entries per mesh across adds/removes/sorts.
- **Depth splits storage from behavior, exactly as WebGPU**: the target owns
  storage (`createDrawTarget(..., { depth: true })`, one buffer shared by
  every entry, cleared once per render), the pipeline keeps behavior
  (`depth`/`depthWrite` = test/write per draw). A depth-testing pipeline
  into a depthless target throws at addDraw - the first pipeline-vs-target
  compatibility check (gpu-review lesson 7's predicted moment).
- **Per-entry params/textures only** - no target-level params on a draw
  target (uniforms are program state; even shared values apply per draw).
  The model matrix lives on the entry. The target-level verbs throw on a
  draw target with a pointer to the per-entry forms.
- **Everything after create is fire-and-forget**, addDraw included: all
  validation state is mirrored UI-side (pipeline uniforms/stride/depth,
  buffer sizes, per-entry bindings), so errors throw at the call site and
  the raster side only warn-and-skips on mirror divergence. That is what
  makes per-frame structural work (the transparent sort, stage 2)
  affordable.
- **No draws array at create**: a target is built by addDraw (sidesteps
  "how do create-time ids come back"); a zero-entry target renders its
  clear color.
- **Single-draw targets became the fixed one-entry case** internally
  (`MeshState.entries`, `fixed: true`, entry id 0): one render path, no
  parallel model. `createShaderTarget`/`createPipelineTexture` surfaces are
  unchanged.

## Surface (flux:gpu -> @solidrt/core/gpu)

    createDrawTarget(w, h, { depth?, clearColor?, render?, loadOp?, filter?, wrap?, label? }) -> TextureId
    addDraw(target, pipeline, params?, { buffer?, textures?, firstVertex?, vertexCount?, instanceCount? }) -> DrawId
    removeDraw(target, draw)
    setDrawParams(target, draw, params)      // the per-object hot path
    setDrawTextures(target, draw, textures)
    setDrawRange(target, draw, update)

Core wraps createDrawTarget with the usual owner-scoped auto-free (entries
die with the target); the verbs re-export raw.

## What stage 1 touched

- alloy: `TargetSpec` split into target-half + `DrawSpec` entry-half;
  `MeshState` = `Vec<DrawEntry>` + target-owned depth + `fixed`;
  `run_pass` takes `PassDraw::Draws` (clear-once, per-entry program/
  uniforms/inputs/VAO/blend/depth, per-unit binding save on first touch);
  new RasterCmds CreateDrawTarget (RPC) + AddDraw/RemoveDraw/
  UpdateDrawParams/UpdateDrawTextures/SetDrawRange (fire-and-forget);
  flush edges = union over entries (`binding_sources`); writeBuffer dirty
  scan = any entry (`reads_buffer`); `shader_sources` mirror keyed
  `(entry, name)`; per-entry `EntryMirror`s with draw bounds; introspection
  gains kind "draws" + a `draws` array (GpuDrawInfo).
- flux gui/gpu.rs: collectors split (`collect_target_half` /
  `collect_entry_half`), six new exports.
- flux-types gui/gpu.d.ts (DrawId brand + docs), core gpu.ts
  (createDrawTarget wrapper + raw re-exports), docs/core.md ("Draw targets"
  section), lattice connection.rs (draws JSON, off-default conventions).
- Examples: alloy/examples/draw_list.rs (headless assertions),
  packages/core/examples/gpu-draw-list.tsx.

## Stage 2 (DONE 2026-08-04): ordering verbs

`before?: DrawId` on addDraw (insert immediately before a live entry) and
`setDrawOrder(target, DrawId[])` - a full permutation of the current ids,
validated by the shared `validate_order` in vocab.rs (one copy: UI mirror
check + raster backstop), fire-and-forget, entry state riding along
untouched. The sorting enabler: opaque front-to-back / transparent
back-to-front, re-issued when the camera moves. Raster-side reorder is a
`sort_by_key` over an id->position map; introspection's draws array
reports the new order automatically (it is list order).

## Deferred / adjacent

- Draws-at-create sugar, target-level shared-params convenience, batched
  updates: demand-gated.
- Per-draw uniform cost (name->location hash per param per draw per render)
  is the known simplified-model cost; the escape hatch (resolve to slots at
  entry creation) is recorded in [gpu-pipeline-object-model](gpu-pipeline-object-model.md)
  item 4 and is invisible from JS. Uniform arrays / UBOs stay open in the
  scene-graph note's tier list.
- Cull/depth-func vocabulary, index buffers, per-instance attributes: still
  [gpu-pipeline-extensions](gpu-pipeline-extensions.md).
