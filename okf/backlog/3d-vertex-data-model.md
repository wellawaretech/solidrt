---
title: Every vertex attribute is a 32-bit float in one immutable interleaved buffer
description: The vertex vocabulary is f32/vec2/vec3/vec4 only and a geometry is one interleaved Float32Array uploaded once, so a color costs 16 bytes where 4 would do, a normal 12 where 4 would do, and a channel that changes every frame re-uploads the channels that do not.
created: 2026-09-11
---

# Every vertex attribute is a 32-bit float in one immutable interleaved buffer

## Symptom

Two limits of the geometry data model, seen from an app that ports a
Three scene or streams its own vertex data:

- **Formats.** `AttrFormat` in `alloy/src/gpu/vocab.rs` is `f32 | vec2 |
  vec3 | vec4`, its GL reflection rejects integer attribute types, and
  `Geometry.vertices` is a `Float32Array` with every stride and slot in
  floats (`FORMAT_FLOATS`). A vertex color is 16 bytes where a
  normalized `rgba8` is 4, a normal 12 where a `snorm16x3` (or an
  octahedral 4) would do, a uv 8 where a half pair is 4. On the mobile
  targets this is vertex bandwidth and memory, on all targets it is
  asset size.
- **Streams and updates.** One interleaved buffer per geometry,
  treated as immutable after creation (`geometry-gpu.ts` uploads once
  and caches by identity). A geometry whose positions change every
  frame (a cloth, a streamed cloud, a growing trail) either rebuilds the
  whole geometry, re-uploading the static channels with it, or moves
  the motion into a texture or instance records, which is the wrong
  shape for per-vertex data. Core already has `writeBuffer`; the 3d
  package only uses it for instance records and mesh styles.

## Comparison

- **Three** keeps one `BufferAttribute` per channel, each its own
  buffer, with `InterleavedBuffer` as the opt-in. The typed array picks
  the format (`Uint8Array`, `Int16Array`, `Float16BufferAttribute`,
  `Float32Array`) and `normalized` says whether integers map to 0..1 /
  -1..1. Updates are `attribute.needsUpdate` with `usage`
  (`DynamicDrawUsage`) and `addUpdateRange`; `setDrawRange` draws a
  slice.
- **Unity** declares a vertex buffer with
  `VertexAttributeDescriptor(attribute, format, dimension, stream)`:
  formats `Float32/Float16/UNorm8/SNorm8/UNorm16/SNorm16/UInt8..SInt32`,
  up to four interleaved streams so a dynamic stream updates alone
  (`SetVertexBufferData` with `MeshUpdateFlags`, `MarkDynamic`).
- **Godot** interleaves into fixed channels (`ARRAY_VERTEX`,
  `ARRAY_NORMAL`, ... `ARRAY_CUSTOM0..3`) split over three buffers
  (vertex, attribute, skin), compresses normals and tangents to 16-bit
  octahedral and uvs to half with `ARRAY_FLAG_COMPRESS_ATTRIBUTES`, and
  gives the custom channels a format list (`RGBA8_UNORM`, `RGBA8_SNORM`,
  `RG_HALF`, `RGBA_HALF`, `R_FLOAT` .. `RGBA_FLOAT`). Updates are
  whole-surface region writes on the `RenderingServer`.

Where SolidRT sits: the interleaved buffer is the deliberate baseline
(Godot's shape, and the direction taken when layouts became open lists
in okf/done/3d-colored-generators.md). The open attribute list with a
format per attribute is already Unity's descriptor minus the format
range and the stream index, so both extensions are additive on that
list. Nothing here is generic enough for core beyond what core already
has (`createBuffer`/`writeBuffer` and the `VertexAttribute` type).

## Done looks like

Stage 1, formats: `VertexAttribute.format` grows past the float
vocabulary with normalized integer and half formats, spelled in one
vocabulary across the stack (alloy `AttrFormat` with the GL type and
normalized flag per format, flux marshalling, the core type, the 3d
layout arithmetic in bytes with attribute alignment). `Geometry.vertices`
becomes an `ArrayBufferView` and every geometry function that indexes
vertices (bounds, transform, fill, merge, the glTF writer, the model file)
reads and writes through the slot's format. The generators keep emitting
f32 (they are the authoring path; compressing is a pass over the
result). The glTF loader passes normalized accessors through instead of
widening them to floats. The check rig proves byte layouts per format
and a round trip through the model file.

Stage 2, streams: a geometry may carry more than one vertex buffer, each
its own interleaved layout, and a stream can be rewritten in place
(`writeBuffer` on the geometry's buffer for that stream) without
touching the others; the entry binds every stream. Three's
`setDrawRange` (a `DrawRange` type exists in core) fits here too. Point
size and a stock points material are a separate, smaller item (the
survey's list, okf/notes/three-feature-survey.md).

## What it involves

Stage 1 is an engine change first: `alloy/src/gpu/vocab.rs` and the GL
attribute pointer setup, `flux/src/alloy_plugins/gpu.rs`,
`packages/core/src/gpu.ts` types, then `packages/3d/src/geometry.ts`
(byte strides, per-format read/write in the slot helpers),
`geometry-gpu.ts`, `gltf.ts`, `model-file.ts` (format version bump) and
the docs. It reaches every function in geometry.ts, which is why the
prefix rule was split off and landed first
(okf/done/non-surface-vertex-layouts.md): its by-name slot reads are
the shape every reader here ends up in.

Stage 2 touches `Geometry` (a streams form beside `vertices`),
`geometry-gpu.ts` acquisition and release, the entry binding in
`scene.ts`, and the picking shape, which reads positions and needs to
know which stream they live in.
