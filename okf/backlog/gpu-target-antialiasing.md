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
step. Impeller anti-aliases its own vector geometry, but a pipeline target
is opaque to it - whatever the app's fragment shader wrote is what gets
composited.

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
  supersampling for free, no API change. What is unverified is the
  minification path: whether the texture node samples with a linear min
  filter, whether mipmaps exist, and what a 2x or 4x downscale actually
  costs in quality. If it is nearest-sampled, the whole technique silently
  does nothing and apps have no way to tell. Worth checking before anything
  else is built.
- **A `samples` option on createPipeline** (2/4/8): allocate a multisampled
  renderbuffer for color and depth, draw into it, and resolve with
  `glBlitFramebuffer` into the texture the id already names. GLES 3.0 has
  both, and the resolve slots in ahead of the existing "render is done" point
  without touching the texture-id contract. On mobile/ANGLE,
  `EXT_multisampled_render_to_texture` does the resolve implicitly and is
  the cheaper path where available.
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
