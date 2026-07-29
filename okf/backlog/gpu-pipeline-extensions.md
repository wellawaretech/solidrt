---
type: backlog-item
title: GPU pipeline extensions
description: Typed (vec, mat4) uniforms, index buffers, float data textures, blending and multi-pass targets on top of the minimal createPipeline.
status: deferred
timestamp: 2026-07-15T00:00:00Z
---

# GPU pipeline extensions

createPipeline (flux:gpu / alloy shader.rs) shipped with the bare minimum a
mesh renderer needs: custom vertex+fragment GLSL, one interleaved float vertex
buffer per pipeline, name-resolved attributes, optional depth buffer, mutable
draw count. Deliberately deferred, in rough order of expected demand:

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
- **Index buffers** (`glDrawElements`): unindexed triangles are fine at small
  scale; indexing pays off once meshes get large or strip-heavy.
- **Float texture formats** (`R32F`/`RGBA32F`) for data textures sampled in
  the vertex stage (e.g. per-sector heights via texelFetch). Workaround:
  fixed-point encode into RGBA8 channels and decode in the shader.
- **Blending toggle** for translucent geometry. Alpha-tested cutouts already
  work via `discard` (depth writes stay correct); true translucency needs
  sorted geometry plus GL blend state on the pipeline. Additive is the case
  that keeps coming up and is the easy half: order-independent, no sorting,
  and it is what soft point splats and glow passes want. Without it,
  `gl_PointSize > 1` draws opaque discs, so a point cloud can only be
  thickened into a scaly overlap, never a smooth field (projects/organism).
- **Raster state**: cull mode and depth func/write are fixed. Two-sided
  shading (`abs(dot(n, l))`) hides the missing cull for now, but a closed
  mesh pays double the fragment work, and depth-write-off is the other half
  of any blended pass.
- **Multiple draw passes into one target** (shared depth buffer, different
  programs). One pipeline + a dynamic buffer region covers the known use
  cases so far.

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
  `collect_params` in flux/src/plugins/gui/texture.rs, hard-wired to
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
What remains here is blending WITHIN one draw.
