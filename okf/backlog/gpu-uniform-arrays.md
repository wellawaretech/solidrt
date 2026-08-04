---
type: backlog-item
title: Uniform arrays (vecN[], mat4[])
description: Array uniforms have no path - the typed-uniform dispatch is single-element only - so a light list or palette becomes N scalar uniforms or a data texture; glUniform*v dispatch by reflected array size is a small extension of the existing path. First consumer is the scene-graph light model.
status: open
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
(bone matrices at scale) still wants float textures (the float-formats
bullet in the extensions file).

Consumers: the scene-graph light model
(../research/scene-graph-3d.md, stage 4) is the named one - a light list
is `vec3[N]` positions and colors - with palettes and small per-object
tables adjacent. Filed 2026-08-04 from that note; no field report asks
yet.
