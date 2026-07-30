---
type: backlog-item
title: GPU pipeline extensions
description: Extensions on top of the minimal createPipeline. Typed uniforms and additive blend/depthWrite landed 2026-07-29; index buffers, float data textures, raster state, alpha translucency and multi-pass targets remain deferred.
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
  scale; indexing pays off once meshes get large or strip-heavy. API shape
  decided in [gpu-review](../analysis/gpu-review.md) (lesson 13): reuse
  `createBuffer` - one buffer kind, no separate index-buffer type (neither
  standard has one) - and the target names `indexBuffer` + `indexFormat:
  "uint16" | "uint32"`. Normalized vertex formats (`unorm8x4` etc.) are the
  adjacent bandwidth item recorded there.
- **Draw range and instancing** (from [gpu-review](../analysis/gpu-review.md)
  lesson 6): `first` for sub-range draws from a shared buffer (the Doom
  dynamic-tail case can draw the tail, not just the first N; trivial) and
  `instanceCount` via `glDrawArraysInstanced` + `gl_InstanceID` (native ES
  3.0), the standard answer to particles and repeated meshes. Both additive
  under the pure-target model - they change what is drawn, not whether the
  pass is idempotent.
- **Float texture formats** (`R32F`/`RGBA32F`) for data textures sampled in
  the vertex stage (e.g. per-sector heights via texelFetch). Workaround:
  fixed-point encode into RGBA8 channels and decode in the shader.
- **Sampleable depth** (from [gpu-review](../analysis/gpu-review.md) lesson
  16): a pipeline's depth is a private renderbuffer, unsampleable by
  construction; both standards make depth a texture (ES 3.0 has depth
  textures and `sampler2DShadow` in core) - the entry ticket to shadow
  maps, depth-of-field, SSAO. The storage swap is small; the open question
  is currency, because a target's id names its colour - its depth needs a
  name of its own to appear in another target's `textures`, after which the
  dependency graph tracks the edge like any other.
- **Blending toggle.** Additive half DONE 2026-07-29: `blend: "add"`
  (`glBlendFunc(ONE, ONE)`) plus the independent `depthWrite: boolean`
  (default true; requires `depth`) on createPipeline/createShaderTarget.
  Explicit by design - the additive-pass recipe is `{ depth: true, blend:
  "add", depthWrite: false }`, written by the app, never inferred (blend does
  NOT imply depth-write off). The clear always writes depth; only the draw
  honors depthWrite. Both reported by get_gpu_resources when off their
  defaults. Still open: true alpha translucency (sorted geometry plus the
  straight-vs-premultiplied question against Impeller's compositing of the
  target). First step regardless: document the target pixel contract
  (premultiplied, non-linear RGBA8 - [gpu-review](../analysis/gpu-review.md)
  lesson 12), which answers the straight-vs-premultiplied half by declaring
  it.
- **Raster state**: cull mode and depth func are fixed (depth WRITE is now an
  option, see blending above). Two-sided shading (`abs(dot(n, l))`) hides the
  missing cull for now, but a closed mesh pays double the fragment work.
- **Multiple draw passes into one target** (shared depth buffer, different
  programs). One pipeline + a dynamic buffer region covers the known use
  cases so far. The object-model blocker is gone: the
  [split](gpu-pipeline-object-model.md) landed 2026-07-30, so draw state now
  lives on RenderPipeline and "N pipelines into one target" has a natural
  shape (a target that takes several pipelines, or targets sharing a depth
  attachment). Cull/depth-func likewise now have their home
  (`PipelineDesc`); each remaining item extends the desc or the target spec,
  not a fused struct. But the blocker moved rather than vanished: pass
  ORDER inside one target breaks the pure-target invariant ("rendering
  twice is indistinguishable from rendering once") that the dirty flush
  relies on, so this is gated on the purity decision in
  [gpu-review](../analysis/gpu-review.md) (structural divergence section:
  recommended shape is manual targets + one `renderTarget` verb + loadOp).
  Do not build this, loadOp, or ping-pong feedback before that decision.

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
