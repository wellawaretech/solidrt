---
type: backlog-item
title: Anti-aliasing for GPU pipeline targets
description: createPipeline targets are single-sample, so any filled geometry has hard jaggies; wanted a sample count (MSAA + resolve) or a documented supersample path with known-good minification.
status: open
timestamp: 2026-07-27T00:00:00Z
---

# Anti-aliasing for GPU pipeline targets

createPipeline renders into a plain single-sample texture. Nothing in the
option bag (params, textures, attributes, buffer, topology, vertexCount,
depth, clearColor) asks for multisampling, and there is no post-resolve
step. The render tree's own geometry gets its AA from the multisampled
target it draws into (Impeller's GL backend has no analytic path AA - see
the comment in alloy/src/gl.rs), but a pipeline target is single-sample
and opaque to all that - whatever the app's fragment shader wrote is what
gets composited.

This is invisible for the shader-toy workload createPipeline shipped for
(full-screen fragment effects have no silhouettes) and invisible for point
clouds, which are stochastically anti-aliased by accident: one-pixel points
at varying density read as a soft gradient. It becomes the dominant artifact
the moment a pipeline draws filled triangles. A rotating mesh silhouette
against a dark background crawls badly, and `discard`-based alpha cutouts
(the sanctioned translucency workaround in
[gpu-pipeline-extensions](gpu-pipeline-extensions.md)) have binary coverage,
so they alias exactly as hard as the geometric edge does.

Evidence: projects/organism draws its flower as 233k one-pixel points
precisely because that is what the API renders well. Converting it to the
triangle mesh it wants to be (the petal is already a closed-form parametric
surface with an analytic outline) is otherwise a strict improvement at the
same vertex count, and anti-aliasing is the thing that makes it a wash.

Candidate answers, cheapest first:

- **Document and support supersampling.** The app already picks its target
  size; rendering at 2x and letting `<texture width="100%">` minify is
  supersampling for free, no API change. The minification path is settled
  in code: pipeline targets are created with TEXTURE_MIN_FILTER = LINEAR
  and no mipmaps (alloy/src/shader.rs), and the texture node draws with
  TextureSampling::Linear. So 2x supersample downsamples cleanly via
  bilinear; 4x undersamples (skips texels) for lack of mips. The technique
  works, but only document 2x as the known-good factor unless mip
  generation is added ([gpu-review](../analysis/gpu-review.md) lesson 15 now
  proposes the shape: `mipmap?: boolean` on SamplerOptions, auto-regen for
  targets off the dirty flush - which would make 4x supersample minify
  correctly too).
- **A `samples` option on createPipeline** (2/4/8): allocate a multisampled
  renderbuffer for color and depth, draw into it, and resolve with
  `glBlitFramebuffer` into the texture the id already names. GLES 3.0 has
  both, and the resolve slots in ahead of the existing "render is done" point
  without touching the texture-id contract. On mobile/ANGLE,
  `EXT_multisampled_render_to_texture` does the resolve implicitly and is
  the cheaper path where available.

  Post-split home (2026-07-30): this bullet predates
  [[gpu-pipeline-object-model]]. `samples` is draw state and belongs on
  `PipelineDesc` (`createRenderPipeline`; the fused `createPipeline`
  forwards), with each target allocating matching MSAA storage - the same
  auto-provisioned pattern as `depth: true`, which
  [gpu-review](../analysis/gpu-review.md) (lesson 7) singles out as the
  mismatch-proof shape: WebGPU makes the app declare `multisample.count` on
  the pipeline AND match it on the attachment, and validates; here the
  target derives its storage from the pipeline, so the mismatch cannot be
  written.
- Interaction with in-place resize: the multisample attachments have to be
  reallocated alongside the target in setShaderSize, same as the depth
  buffer.

Do not write the MSAA path from scratch: alloy/src/gl.rs `draw_offscreen`
already implements exactly this for snapshot repaint boundaries - a
multisampled color renderbuffer plus a multisampled DEPTH24_STENCIL8
renderbuffer, resolved into the single-sample target with `glBlitFramebuffer`,
including the retry that drops to single-sample when a driver advertises
MAX_SAMPLES but rejects the config. Pipeline targets attach a bare texture to
COLOR_ATTACHMENT0 with no multisampling (alloy/src/shader.rs), which is the
whole of the difference. If the shared MSAA scratch rig in
snapshot-offscreen-rig-churn.md happens, pipeline targets should draw from the
same pool rather than allocating their own.

Related: wide lines are not an escape hatch here. `topology: "lines"` is
capped at one pixel in GLES core, so anything thicker has to be expanded
into triangles by the app - which lands right back on this item.
