---
type: backlog-item
title: GPU pipeline extensions
description: "Done as a container 2026-08-11: every decided extension landed (typed uniforms + additive blend/depthWrite 2026-07-29, draw range + instancing 2026-07-30, multi-pass draw targets, index buffers, cull mode, per-instance attributes 2026-08-04) and the four remaining opens were split into their own items - gpu-float-texture-formats, gpu-sampleable-depth, gpu-alpha-translucency, gpu-depth-func. This file is the record of what landed."
status: done
timestamp: 2026-07-15T00:00:00Z
---

# GPU pipeline extensions

createPipeline (flux:gpu / alloy shader.rs) shipped with the bare minimum a
mesh renderer needs: custom vertex+fragment GLSL, one interleaved float vertex
buffer per pipeline, name-resolved attributes, optional depth buffer, mutable
draw count. This item collected the deliberately deferred extensions; every
decided one has landed (records below), and on 2026-08-11 the four still-open
bullets were split into their own items, because this one file kept being the
destination for unrelated asks:

- [gpu-float-texture-formats](gpu-float-texture-formats.md) - R32F/RGBA32F
  data textures (was the float-formats bullet).
- [gpu-sampleable-depth](gpu-sampleable-depth.md) - depth as a sampleable,
  nameable texture (was the sampleable-depth bullet).
- [gpu-alpha-translucency](gpu-alpha-translucency.md) - sorted alpha
  blending (was the open half of the blending bullet).
- [gpu-depth-func](gpu-depth-func.md) - depthCompare option (was the open
  half of the raster-state bullet).

The landed extensions, in the order demand arrived:

- **Typed uniforms (vecN/mat4).** DONE 2026-07-29. A param value is `number |
  number[]` end to end (`alloy::ParamValue` Scalar/Array), dispatched by the
  reflected GL uniform type in `apply_uniform` (shader.rs): float/int/bool
  scalars, `uniform{2,3,4}f`, `uniformMatrix4fv` (16, column-major). A
  component count that does not fit the declared type warns and skips at
  render (no JS call site left to throw at). `iResolution` declared vec3 in a
  raw source (the Shadertoy shape) is filled as `(w, h, 1.0)`. Ripples
  covered: both flux marshal sites, rendertree texture params, window shader
  params, gpu-resources JSON (scalar vs array), flux-types + core types +
  docs/core.md.
- **Index buffers.** DONE 2026-08-04, lesson 13's decided shape: reuse
  `createBuffer` (one buffer kind, as in both standards) + per-entry
  `indexBuffer` + `indexFormat: "uint16" | "uint32"` on addDraw AND the
  single-draw creates (the shared entry collector). The draw becomes
  `glDrawElements`; the range switches to WebGPU's indexed spelling -
  `firstIndex`/`indexCount` - and the vertex-named pair throws on an indexed
  entry (and vice versa, in setDraw/setDrawRange too: `DrawRange::merged`
  owns the rule), so a range never silently counts the wrong unit. Bounds
  check runs against the INDEX buffer at the format's element size; index
  VALUES are not checked against the vertex buffer (that would mean reading
  them back - documented as GL's undefined fetch). The element-array binding
  is VAO state, captured once in `build_vao`; `writeBuffer` into an index
  buffer re-renders targets indexing through it (`reads_buffer` covers both
  roles). No base vertex (ES 3.2); demand signal was live: the-third-dimension
  logged 49926 vertices for 16642 triangles - exactly 3 per triangle, zero
  sharing. Normalized vertex formats (`unorm8x4` etc.) remain the adjacent
  bandwidth item.
- **Draw range and instancing.** DONE 2026-07-30. The draw is one value,
  WebGPU-shaped: `firstVertex` + `vertexCount` + `instanceCount` on the
  target spec (`alloy::DrawRange`), drawn via `glDrawArraysInstanced` when
  instanceCount != 1 (1 keeps the plain `glDrawArrays`, bit-identical to
  before; 0 draws nothing). `setDrawCount` is REPLACED by `setDraw(id, {
  firstVertex?, vertexCount?, instanceCount? })` with params-style partial
  merge (`DrawUpdate`, merged against the `TargetMirror`). The
  whole-buffer-derivation rule moved UI-side (`resolve_draw_range` in
  vocab.rs, one copy for create + update); raster's `resolve_target_mesh`
  shrank to a buffer lookup, so "buffer N not found" and range errors all
  throw at the JS call site. gl_VertexID includes firstVertex; gl_InstanceID
  counts from 0 (no base instance in ES 3.0 - `firstInstance` deliberately
  not offered). get_gpu_resources reports firstVertex/instanceCount off
  their 0/1 defaults. Example: packages/core/examples/gpu-instancing.tsx.
- **Per-instance attributes** (vertex divisor). DONE 2026-08-04, the
  sketched shape: `instanceAttributes: [{name, format}]` on the pipeline
  desc plus `instanceBuffer` per entry (WebGPU's `stepMode: "instance"`;
  `build_vao` records the second layout at divisor 1 - divisor is VAO state
  in ES 3.0, so `run_pass` needed zero changes). The two layouts share one
  attribute namespace; a name in both throws at pipeline creation
  (`RenderPipeline::new`, inside the blocking create RPC). Pairing is a
  contract: declared instanceAttributes require an entry instanceBuffer and
  vice versa (`resolve_entry_range` UI-side, `check_entry_buffers` raster
  backstop). `instanceCount` gained the fetch bound against the instance
  buffer AND the whole-buffer derivation rule: omitted = one instance per
  record when an instance buffer is bound, else 1 (DrawRange's default
  instance_count is now the -1 sentinel, resolved like vertex_count). Two
  consolidations rode along, deliberately breaking internal signatures: the
  mirrors' `(draw_bound, indexed)` pair became `vocab::DrawBounds`
  (fetch/indexed/instance, one value validate_draw_range takes), and the
  raster-side buffer trio became `target::EntryBuffers`
  (vertex/index/instance Rc + id, one resolve_entry_buffers).
  `reads_buffer` covers the third role; introspection reports
  `instanceBuffer` per entry/flat target and `instanceAttributes` on render
  pipelines. No matrix attribute formats (a mat4 is four vec4 columns, as
  in WebGPU) and no base instance (unchanged, ES 3.0). Probe-pinned in
  alloy/examples/draw_instanced.rs (20 assertions incl. attributeless +
  instanced and index + instance combined); gpu-instancing.tsx converted to
  records.
- **Blending toggle.** Additive half DONE 2026-07-29: `blend: "add"`
  (`glBlendFunc(ONE, ONE)`) plus the independent `depthWrite: boolean`
  (default true; requires `depth`) on createPipeline/createShaderTarget.
  Explicit by design - the additive-pass recipe is `{ depth: true, blend:
  "add", depthWrite: false }`, written by the app, never inferred (blend does
  NOT imply depth-write off). The clear always writes depth; only the draw
  honors depthWrite. Both reported by get_gpu_resources when off their
  defaults. The open half - true alpha translucency - split to
  [gpu-alpha-translucency](gpu-alpha-translucency.md).
- **Raster state**: cull mode DONE 2026-08-04 - `cull: "none" | "back" |
  "front"` on createRenderPipeline, per entry in `run_pass` with cull
  face/winding in the save/restore set. The winding rule is deliberately
  WebGPU's framebuffer-space one, NOT GL's raw default: front =
  counter-clockwise AS DISPLAYED, which pins `glFrontFace(CW)` because the
  displayed image is the y flip of GL window space. Chosen for the standard
  rig: a CCW-front mesh through a right-handed camera (looking down -z)
  with the usual y negation for display culls correctly with "back"
  (probe-pinned in alloy/examples/draw_indexed.rs). Caveat learned live: a
  left-handed DIY rig - camera looking toward +z without mirroring x, which
  gpu-pipeline.tsx originally did - mirrors the winding and shows the mesh
  interior; the fix is the rig, and the example now carries the textbook
  one. The open half - the depth comparison - split to
  [gpu-depth-func](gpu-depth-func.md).
- **Multiple draw passes into one target.** DONE 2026-08-04 (stages 1+2) as
  [gpu-draw-list](gpu-draw-list.md): `createDrawTarget` holds a retained,
  ordered, mutable draw list (addDraw/removeDraw with stable DrawIds,
  per-entry setDrawParams/setDrawTextures/setDrawRange), rendered clear-once
  + entries-in-order as ONE pass; depth storage is target-owned, depth
  behavior stays pipeline state. The purity worry resolved as the
  scene-graph note argued: a retained list is input data, so the feature is
  legal on flush-rendered targets and the dirty flush is untouched.
  Ordering verbs (before on addDraw, setDrawOrder) landed as stage 2 the
  same day; verified headless + live (Linux, Android TV).

Adjacent, filed separately because they are not createPipeline options:
[anti-aliasing for pipeline targets](gpu-target-antialiasing.md),
[paint properties on the texture element](texture-element-compositing.md),
[sampler filter and wrap state](gpu-sampler-state.md) and
[dependency propagation between targets](gpu-target-dependency-propagation.md).

## Demand signal (2026-07-29)

Two field reports against 0.0.39 independently ranked items from the list above
as their top ask, which is worth recording because both were written by people
building real apps rather than reviewing the API.

- **Typed uniforms** is named the highest-leverage change by BOTH
  projects/shadertoy (#2) and projects/second-reality (#5). Every vec2 centre,
  vec3 camera or palette phase becomes N scalar uniforms plus a reassembly
  macro:

      uniform float iResX;
      uniform float iResY;
      #define iResolution vec3(iResX, iResY, 1.0)

  Shadertoy's contract is `vec3 iResolution`, `vec4 iMouse`, `vec4 iDate`,
  `int iFrame` - none settable - so an entire compat block exists purely to
  work around this. Landing it lets unmodified Shadertoy, GLSL Sandbox and Book
  of Shaders code run as-is. The marshalling site is
  `collect_params` in flux/src/plugins/gui/gpu.rs, hard-wired to
  `Vec<(String, f32)>` end to end.

- **Blending toggle** now has three independent requesters: projects/organism
  (point splats, noted above), projects/second-reality (glenz vectors), and
  projects/shadertoy implicitly. Second-reality shipped a convex-only
  workaround - front/back faces split into two targets composited with
  `<texture blendMode="plus">` - which works only because a convex object has
  exactly one front and one back face per pixel. Non-convex transparent meshes
  and many-particle additive accumulation with per-particle colour still have no
  path.

Note the tree-level compositing half is DONE
([texture-element-compositing](texture-element-compositing.md)) and documented
as of 2026-07-29, with an example in packages/core/examples/gpu-texture-blend.tsx.
Blending WITHIN one draw landed the same day (additive only, see the blending
bullet above); both top demand-signal items are now closed.
