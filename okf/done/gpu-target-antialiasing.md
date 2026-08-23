---
title: Anti-aliasing for GPU pipeline targets
description: Mesh targets were single-sample, so any filled geometry had hard jaggies. Landed 2026-08-23 as a target-level `samples` option (createShaderTarget, createPipelineTexture, createDrawTarget, and `<Scene samples>` in @solidrt/3d) with two engine flavors - in-tile via EXT_multisampled_render_to_texture, explicit MSAA renderbuffer + resolve blit elsewhere - verified on Linux; the in-tile flavor awaits an Android run.
created: 2026-07-27
completed: 2026-08-23
---

# Anti-aliasing for GPU pipeline targets

## Landed 2026-08-23: `samples` on the target

`samples?: 1 | 2 | 4 | 8` on the target half of every mesh-target create
(`createShaderTarget`, `createPipelineTexture`, `createDrawTarget`) and
forwarded as `<Scene samples>` / `SceneOptions.samples` in `@solidrt/3d`.
Fragment targets (fullscreen, no silhouettes) do not take it.

Placement differs from the PipelineDesc shape sketched below, deliberately:
that sketch predates draw targets. One draw target holds entries with many
pipelines, and multisampling is pure storage - it changes no program or
raster state - so it follows depth's rule ("explicit on the draw target")
and lives in `TargetSpec` next to `clear_color`/`sampler`. The
mismatch-proof property the note wanted survives: storage derives from the
one declaration, nothing can disagree with it.

Engine (`alloy/src/gpu/target.rs`, `Msaa`): the target texture stays
single-sample in both flavors, so the id keeps meaning the resolved output
and display, sampling, `readTexture`, `copyTexture` and the dependency
graph are untouched.

- `InTile`: EXT_multisampled_render_to_texture, where advertised (tiled
  mobile GPUs; the window path's `MsrttFns` is reused). The texture itself
  is attached with a sample count and the driver resolves at tile
  writeback; depth is allocated through the extension's storage call. No
  extra color storage, no resolve pass.
- `Explicit`: ES 3.0 core. A multisampled color renderbuffer in its own
  FBO, resolved into the texture with one `glBlitFramebuffer` after every
  pass and clear, then invalidated. Depth multisampled to match.

Clamped to `MAX_SAMPLES`; a configuration the driver refuses falls back to
single-sample with a warning rather than failing the create (the app asked
for quality, not a requirement); the effective count is reported in the
resource inventory (`/gpu` `samples`). Resize reallocates the multisample
storage alongside the texture, with the same rollback. `loadOp: "load"` +
`samples > 1` throws: ES 3.0 cannot blit single-sample contents into
multisampled storage, and the extension defines the previous contents as
undefined, so accumulation targets stay single-sample.

Verified on Linux (explicit flavor) via the control API with a static tilted
cube in two scenes: the single-sample readback has 2 gray levels, the 4x one
5 (0, 1/4, 2/4, 3/4, 1) with ~450 coverage-weighted edge pixels at 256x256;
after a resize to 320 the 4x target still reads 5 levels. Probe:
`probes/msaa-probe.tsx`.

Not done, on purpose: sharing multisample scratch across targets (the
snapshot-rig pool idea below) - each target owns its storage, which is
simple and only costs memory proportional to what the app asked for. The
in-tile flavor has only been compiled, not run, on a device that has the
extension; an Android run is the remaining verification.

## Original item


createPipeline renders into a plain single-sample texture. Nothing in the
option bag (params, textures, attributes, buffer, topology, vertexCount,
depth, clearColor) asks for multisampling, and there is no post-resolve
step. The render tree's own geometry gets its AA from the multisampled
target it draws into (Impeller's GL backend has no analytic path AA - see
the comment in alloy/src/gl/rig.rs), but a pipeline target is single-sample
and opaque to all that - whatever the app's fragment shader wrote is what
gets composited.

This is invisible for the shader-toy workload createPipeline shipped for
(full-screen fragment effects have no silhouettes) and invisible for point
clouds, which are stochastically anti-aliased by accident: one-pixel points
at varying density read as a soft gradient. It becomes the dominant artifact
the moment a pipeline draws filled triangles. A rotating mesh silhouette
against a dark background crawls badly, and `discard`-based alpha cutouts
(the sanctioned translucency workaround in
[gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md)) have binary coverage,
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
  generation is added ([gpu-review](../notes/gpu-review.md) lesson 15 now
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
  [gpu-review](../notes/gpu-review.md) (lesson 7) singles out as the
  mismatch-proof shape: WebGPU makes the app declare `multisample.count` on
  the pipeline AND match it on the attachment, and validates; here the
  target derives its storage from the pipeline, so the mismatch cannot be
  written.
- Interaction with in-place resize: the multisample attachments have to be
  reallocated alongside the target in setShaderSize, same as the depth
  buffer.

Do not write the MSAA path from scratch: alloy/src/gl/draw.rs `draw_offscreen`
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
