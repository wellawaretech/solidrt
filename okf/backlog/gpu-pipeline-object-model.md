---
type: backlog-item
title: Split GPU pipeline state from the render target
description: Draw state (topology, depth, blend, cull) is fused into the render target, which blocks multi-pass and makes every new pipeline option a dozen-site edit; plus the spec-struct duplication, unrepresentable-state and coverage gaps found in the same review.
status: done
timestamp: 2026-07-29T00:00:00Z
---

# Split GPU pipeline state from the render target

Written as a retrospective right after typed uniforms and the additive blend
toggle landed ([gpu-pipeline-extensions](gpu-pipeline-extensions.md),
2026-07-29). Both stages were implemented as locally minimal changes that fit
the existing shapes. The shapes are the problem: each of the items below is
something the change would have done differently with a free hand, and most of
them get more expensive with every further extension bolted onto the current
model.

Nothing here is a correctness bug in what shipped (one silent-drop exception,
noted below). It is all structure.

**DONE 2026-07-30.** Items 1-3 landed as one restructuring; item 4's first
two bullets are done (the first was already in place), the hot-path bullet
stays deliberately unpicked; item 5 shipped first as the safety net; item 6's
parity automation is already a candidate in
[release-readiness-checks](release-readiness-checks.md). Verified three ways:
`alloy/examples/pipeline_blend.rs` (fused + split paths pixel-match,
headless), `alloy/examples/shader_uniforms.rs` (typed dispatch + params
merge), and `flux/examples/gpu_split.rs` (the whole JS surface end to end,
including the boundary throws). What shipped, per layer:

- alloy: `RenderPipeline` (program + `PipelineDesc` draw state: typed
  `attributes`, `Topology` enum, `Option<BlendMode>`,
  `Option<DepthState { write }>` - the invalid depth/depthWrite combination
  is now unrepresentable and its runtime check deleted). New id space with
  `create_render_pipeline`/`destroy_render_pipeline`;
  `create_shader_target(pipeline, TargetSpec)` takes the per-target half
  (size, params, textures, buffer, draw_count, clearColor, sampler). The
  2x2 spec structs collapsed to one owned `TargetSpec` + nesting
  `PipelineSpec` used by both the public API and the raster channel.
- Dead code found and removed on the way: registered programs were ALWAYS
  pipeline-kind (`linkProgram` requires both stages), so the fragment branch
  of the old create_shader_target and the hand-maintained defaults check
  guarded nothing; `program_kinds`/`GpuProgramInfo.kind` reduced accordingly.
- flux: `createRenderPipeline`/`destroyRenderPipeline`; vocabulary
  (blend/topology/attr formats) parses at the JS boundary so bad words throw
  at the call site; `createShaderTarget` throws on draw-state keys with a
  pointer to createRenderPipeline (the silent-drop class, closed). Fused
  `createShader`/`createPipeline` surfaces unchanged.
- Introspection: targets report `pipelineId`; registered pipelines listed as
  `renderPipelines` in get_gpu_resources.
- Docs: flux-types gpu.d.ts, core gpu.ts, docs/core.md, examples README,
  gpu-raw-program.tsx updated (docs/flux.md does not cover flux:gpu; scaffold
  AGENTS.md and core types.d.ts references were program-level and stand).

Deferred with their new home ready: index buffers, float data textures,
cull/depth-func, multi-pass targets - see
[gpu-pipeline-extensions](gpu-pipeline-extensions.md).

## 1. Draw state belongs to a pipeline, not to a target

`ShaderTexture` fuses four things: the FBO plus target texture, the program,
the vertex layout, and the draw state (topology, depth, depthWrite, blend,
clearColor). GL, WebGPU and Vulkan all put that last group in a pipeline state
object next to the program, and the raw layer already exposed
(`compileShader` -> `linkProgram` -> `createShaderTarget`) is halfway there.

    createRenderPipeline(program, { attributes, topology, blend, depth, depthWrite, cull })
    createShaderTarget(pipeline, w, h, { params, textures, buffer, clearColor })

Two things fall out. The "pipeline options given, but the program is a
fragment shader" error disappears, because a fragment program is never handed
to `createRenderPipeline`. And **multi-pass targets stop being blocked on the
object model**: several passes into one target sharing a depth buffer is
exactly "one target, N pipeline states", which the fused struct cannot
express. Cull mode and depth func, also deferred, are draw state with the same
obvious home.

That is the sequencing argument for doing this before the rest of
[gpu-pipeline-extensions](gpu-pipeline-extensions.md): index buffers, float
data textures, cull/depth-func and multi-pass each add more fields to the
fused struct if the split has not happened first.

## 2. Make the invalid states unrepresentable

`depth: bool` plus `depth_write: bool` is four states, three of them valid,
and the fourth is rejected by a hand-written runtime check with its own error
message. `depth: Option<DepthState { write: bool }>` is three states, three
valid, and needs no check at all.

The same shape applies one level up. This defaults comparison in
`Context::create_shader_target` (alloy/src/context.rs) is a hand-maintained
list that every future option must remember to join:

    if !is_pipeline
      && (!spec.attributes.is_empty()
        || spec.buffer_id != 0
        || spec.depth
        || !spec.depth_write
        || spec.blend != "none"
        || spec.draw_count >= 0
        || spec.topology != "triangles")

Forgetting to extend it is silent: the option is accepted and ignored. It is
already incomplete - `clearColor` is a mesh-only field (a fragment target's
covering triangle writes every pixel, so `from_fragment_program` does not even
take one) and is absent from the list, so
`createShaderTarget(fragmentProgram, w, h, { clearColor: [1, 0, 0, 1] })` is
accepted and silently dropped today. Minor in isolation; it is the bug class
that matters.

A `TargetKind::Fragment | TargetKind::Mesh(MeshSpec)` enum deletes the check
and the class together. The JS boundary still wants one "these keys are
meaningless for this program kind" check, but at the boundary, where it can
throw at the call site.

## 3. Collapse the four spec structs

`PipelineSpec` / `TargetSpec` (context.rs) and `PipelineSpecOwned` /
`TargetSpecOwned` (raster.rs) are a 2x2 of (with sources, without) x
(borrowed, owned). Adding `blend` and `depthWrite` meant editing four struct
definitions, two constructors that are now 15 and 16 positional arguments
behind `#[allow(clippy::too_many_arguments)]`, the flux decode struct, and
every construction and introspection site: well over a dozen edits for two
concepts.

It also grew two adjacent bools in a positional signature
(`..., depth: bool, depth_write: bool, blend, ...`), which transposes silently
at a call site. One owned spec carrying
`program: Program::Compile { vs, fs } | Program::Linked(id)`, passed by
reference into the constructors, makes a change like this shrink the
signatures instead of growing them.

## 4. Adjacent smaller fixes

- **`last_params` should merge, not replace.** DONE (was already in place by
  the time this item was picked up: `ShaderTexture::merge_params` folds
  updates by name, and it is `last_params`' only writer). The merge contract
  is now asserted headlessly by `alloy/examples/shader_uniforms.rs`.
- **Parse the vocabulary at the JS boundary.** `blend`, `topology` and
  attribute formats all cross as strings and are parsed on the raster thread,
  so `blend: "addd"` fails through an RPC reply instead of at the
  `createPipeline` call. Having the flux layer call `alloy::BlendMode::parse`
  keeps the vocabulary owned by alloy (still marshalling, not domain logic)
  while the error lands where `Exception::throw_message` can point at the call
  site.
- **Hot path, only if it ever matters.** Per-frame uniform writes allocate a
  `String` per param and hash it per uniform per render. Resolving names to
  locations once at target creation and carrying `(slot, ParamValue)` is the
  real redesign, and it is invisible from JS. Noting it because `ParamValue`'s
  enum-over-Vec choice is decorative next to it. Not worth doing on spec.

## 5. Not an interface change: headless assertion coverage

DONE (2026-07-30). Two self-asserting examples in the `texture_paint.rs`
mold, both run-verified:

- `alloy/examples/pipeline_blend.rs`: two overlapping additive splats;
  asserts the overlap pixel is the sum of both (brighter than either alone),
  the "none" control overwrites, and background pixels carry the clear color.
- `alloy/examples/shader_uniforms.rs`: four bands pinning the full typed
  dispatch table (float and int scalars, vec2/vec3/vec4, mat4 in column-major
  order - the rotation matrix catches a transposed upload), plus a partial
  params update asserting merge-by-name semantics.

## 6. One doc-surface note

The blend/uniform contract is now restated by hand in flux-types
`gui/gpu.d.ts`, core `types.d.ts`, core `gpu.ts` doc comments,
`docs/core.md`, scaffold `AGENTS.md` and `packages/core/examples/README.md`.
That is why the examples README was missed on the first pass and had to be
caught by review. Generating `docs/flux.md` from the `.d.ts`, or adding a
surface-parity check to the gate in
[release-readiness-checks](release-readiness-checks.md), turns that vigilance
into a build failure.
