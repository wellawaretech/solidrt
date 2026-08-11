---
type: backlog-item
title: Uniform arrays (vecN[], mat4[])
description: "Done 2026-08-11: a declared array uniform takes one flat count*components param under its bare name via the glUniform*v forms; reflection unified on one typed UniformSlot (kind + count) driving validation and dispatch alike. Sampler arrays deliberately unsupported; large data stays with float textures."
status: done
timestamp: 2026-08-04T00:00:00Z
---

# Uniform arrays

Typed uniforms ([gpu-pipeline-extensions](gpu-pipeline-extensions.md))
dispatch a `number | number[]` param by the reflected GL uniform type -
float/int/bool scalars, vec2/3/4, mat4 - but strictly one element each: a
mat4 is exactly 16 floats. A declared array (`uniform vec3
uLightColor[4];`) has no path: GL reflection reports it as its element
type with a size the mirrored name -> (kind, components) table does not
carry, and names it `uLightColor[0]`, so any write fails call-site
validation (unknown name or wrong length). Per-kind lists today mean N
separate scalar/vec uniforms with the cap baked into shader-source names,
or a fixed-point-encoded RGBA8 data texture.

The shape: accept a flat `number[]` of length `size * components` for an
array uniform and dispatch through the `v` forms (`glUniform{1,2,3,4}fv`,
`glUniformMatrix4fv`) with the reflected element count. Design details:
fold the array size into the mirrored table (strip the `[0]` suffix,
carry the count) so call-site validation keeps throwing on wrong lengths,
and let the introspection JSON (already scalar-vs-array aware) report the
element count. ES 3.0 guarantees a few hundred vec4-equivalents per
stage, so light lists and palettes fit comfortably; genuinely large data
(bone matrices at scale) still wants float textures
([gpu-float-texture-formats](gpu-float-texture-formats.md)).

Consumers: the scene-graph light model
(../research/scene-graph-3d.md, stage 4) is the named one - a light list
is `vec3[N]` positions and colors - with palettes and small per-object
tables adjacent. Filed 2026-08-04 from that note; no field report asks
yet.

Implemented 2026-08-11, one step more global than the sketch above: the
raw-utype half of reflection was deleted rather than extended. A single
`UniformSlot { kind, count }` (vocab.rs) is now the reflection currency
on both sides - `ShaderProgram.uniforms` stores `(location, slot)`,
`UniformKind::from_gl` runs once at reflection, and `pass::apply_uniform`
dispatches on the slot through the `v` slice forms, so validation and
dispatch compute the expected length from the same place. The `[0]`
suffix is stripped at reflection; a param supplies `count * components`
floats flat; errors spell arrays as `vec3[4] (expects 12)`.
Deliberately unsupported: sampler2D arrays (a texture binding names one
unit; rejected when named), int vectors, non-mat4 matrices. Large data
(bone matrices at scale) stays with
[gpu-float-texture-formats](gpu-float-texture-formats.md).
