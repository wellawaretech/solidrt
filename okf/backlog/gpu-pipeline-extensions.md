---
type: backlog-item
title: GPU pipeline extensions (typed uniforms, index buffers, float textures, blending)
status: deferred
timestamp: 2026-07-15T00:00:00Z
---

# GPU pipeline extensions

createPipeline (flux:gpu / alloy shader.rs) shipped with the bare minimum a
mesh renderer needs: custom vertex+fragment GLSL, one interleaved float vertex
buffer per pipeline, name-resolved attributes, optional depth buffer, mutable
draw count. Deliberately deferred, in rough order of expected demand:

- **Typed uniforms (vecN/mat4).** Params are float scalars end to end
  (rendertree texture node params, flux marshalling, alloy `uniform_1_f32`).
  Reflection already walks active uniforms; also record each uniform's GL type
  and dispatch `uniform{2,3,4}f` / `uniformMatrix4fv` from `number[]` values.
  Ripples through the `<texture params>` prop type, rendertree
  `Vec<(String, f32)>`, and both flux marshal sites. Until then, shaders
  compute matrices from scalars in the vertex stage (fine for e.g. a camera
  pose).
- **Index buffers** (`glDrawElements`): unindexed triangles are fine at small
  scale; indexing pays off once meshes get large or strip-heavy.
- **Float texture formats** (`R32F`/`RGBA32F`) for data textures sampled in
  the vertex stage (e.g. per-sector heights via texelFetch). Workaround:
  fixed-point encode into RGBA8 channels and decode in the shader.
- **Blending toggle** for translucent geometry. Alpha-tested cutouts already
  work via `discard` (depth writes stay correct); true translucency needs
  sorted geometry plus GL blend state on the pipeline.
- **Multiple draw passes into one target** (shared depth buffer, different
  programs). One pipeline + a dynamic buffer region covers the known use
  cases so far.
