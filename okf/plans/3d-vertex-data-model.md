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

## Stage 1 vocabulary

The format names a `VertexAttribute` (and an `InstanceAttribute`, which
shares the table) may carry. WebGPU's `GPUVertexFormat` spelling, the
reference the rest of vocab.rs already follows (index formats, draw
ranges), which also renames today's four float words: `f32 | vec2 |
vec3 | vec4` become `float32 | float32x2 | float32x3 | float32x4`. Those
were the shader's GLSL types; from here on a format names the BYTES in
the buffer and the shader type is derived from it, so the two want
different spellings.

| format | bytes | GL type | normalized | shader `in` |
| --- | --- | --- | --- | --- |
| float32 | 4 | FLOAT | no | float |
| float32x2 | 8 | FLOAT | no | vec2 |
| float32x3 | 12 | FLOAT | no | vec3 |
| float32x4 | 16 | FLOAT | no | vec4 |
| float16x2 | 4 | HALF_FLOAT | no | vec2 |
| float16x4 | 8 | HALF_FLOAT | no | vec4 |
| unorm8x4 | 4 | UNSIGNED_BYTE | yes | vec4 |
| snorm8x4 | 4 | BYTE | yes | vec4 |
| unorm16x2 | 4 | UNSIGNED_SHORT | yes | vec2 |
| unorm16x4 | 8 | UNSIGNED_SHORT | yes | vec4 |
| snorm16x2 | 4 | SHORT | yes | vec2 |
| snorm16x4 | 8 | SHORT | yes | vec4 |
| uint8x4 | 4 | UNSIGNED_BYTE | no | vec4 |
| uint16x2 | 4 | UNSIGNED_SHORT | no | vec2 |
| uint16x4 | 8 | UNSIGNED_SHORT | no | vec4 |

Rules the table is built on:

- **Every format is a multiple of 4 bytes.** Then every offset and every
  stride is 4-aligned by construction (WebGPU's stride rule, Unity's
  stream rule), no padding logic exists anywhere, and a `Float32Array`
  view over any vertex buffer is always valid, which is how the picking
  shape keeps reading positions. This is why the 8-bit formats come in
  x4 only and the 16-bit ones in x2 and x4: WebGPU's `unorm8x2` is 2
  bytes and would break the rule for one channel shape none of the three
  engines' fixed channels use (Godot has no RG8 either).
- **The shader side stays float in stage 1.** Every format feeds a
  float-typed `in`; the GL attribute pointer converts (normalized or
  not) on fetch. An unnormalized integer format (joint indices) arrives
  as the exact integer in a float, so `uint8x4` feeds `in vec4 aJoints`
  unchanged. `AttrFormat::from_gl` and the link-time rejection of
  integer and matrix `in`s do not change; program reflection keeps
  reporting the float32xN the shader declares. A pipeline attribute
  matches a reflected attribute by NAME and COMPONENT COUNT, no longer
  by format equality; the same rule replaces the exact-format compare
  in `missingAttributes` (material.ts). This is a stage boundary, not
  the design: the table is built with `gl_type` and `normalized` per
  row so a later stage adds a kind column (float or integer), integer
  `in`s fed through the integer attribute pointer, and the 32-bit
  integer rows, all additive.
- **Left out on purpose.** Signed unnormalized integers (`sint8`,
  `sint16`): Godot's custom-format list has none and a float `in` holds
  them no better than a normalized form; add a row when a channel needs
  one. 32-bit integers: not exactly representable in a float `in`; they
  come with the integer-input stage above. Packed 10-10-10-2: none of
  the three engines exposes it on the vertex API.

The engine side of the table is one place, `AttrFormat` in vocab.rs:
`parse`/`name` over the spellings above, `components`, `bytes`,
`gl_type`, `normalized`. `vertex_stride` and `instance_strides` sum
`bytes`; `record_layout` (gl/entry.rs) passes `gl_type` and `normalized`
to the attribute pointer and advances by `bytes`. flux's marshalling
parses through the same function, so it does not change; the
`flux:gpu` type union widens and core re-exports it.

## Stage 1 in the 3d package

- `layoutStride` and `layoutSlot(...).offset` are BYTES. `Geometry.
  vertices` is an `ArrayBufferView` (a `Float32Array` from the
  generators, a `Uint8Array` view from the loaders), 4-aligned, byte
  length a multiple of the stride (`validateGeometry`).
- One codec table per format (`bytes`, `components`, `read(dv, at, k)`,
  `write(dv, at, k, v)`) over a `DataView`. Half floats use
  `DataView.getFloat16/setFloat16`, present in both bun and flux
  (checked 2026-09-11). Unorm reads divide by the type's max, snorm
  reads clamp `v / max` at -1 (the GL ES 3.0 mapping); writes round the
  clamped value.
- Callers never see offsets or strides. A function that indexes
  vertices (bounds, transform, fill, withAttribute, merge,
  wireframe/edges, fillColors) asks the geometry for a reader or writer
  bound to an attribute name and only ever passes a vertex index and a
  component. The accessor owns the buffer, the slot and the codec, so
  stage 2 (an attribute living in one of several streams) changes the
  inside of the accessor and no caller.
- The first attribute is `aPos float32x3`, the rule that landed in
  okf/done/non-surface-vertex-layouts.md with the new spelling.
  Generator layouts stay float-only in addition to the prefix
  (`checkGeneratorLayout`): a generator writes a `Float32Array` with
  float indexing, and compressing is `withAttribute(name, "unorm8x4",
  fill)` over the result, not a generator option. The presets
  (`standard`, `colored`, `skinned`) stay float; Three's default
  authoring shape.
- Picking: `createShape` gets a `Float32Array` view over the vertex
  bytes at stride/4, valid by the 4-byte rule.
- glTF: an accessor passes through when (componentType, normalized,
  element count) maps exactly onto a row of the table; anything else
  widens to float32xN as today. Joint bounds and flat-shading keep
  their own widened copies for the computation; pass-through only
  decides what lands in the vertex buffer.
- Model file: version 6. The vertices block is already raw bytes;
  `vertexCount` divides by the byte stride, decode returns a
  `Uint8Array` view, `decodeLayout` checks names against the codec
  table.
- Rename fallout for the four float words: format literals in
  geometry.ts, glsl.ts, material.ts, the 3d and core examples and
  checks, 2d shaders.ts, core AGENTS.md and 3d AGENTS.md, the flux
  gpu examples, and the flux-types union.

Done looks like: the check rig writes every format through the codec
and proves the raw bytes, transforms and merges a packed geometry, and
round-trips one through the model file; `bunx srt check packages/3d`
passes; the alloy tests cover `vertex_stride` over packed formats and
the component-count match.

## Stage 2

Streams: a geometry may carry more than one vertex buffer, each its own
interleaved layout, and a stream can be rewritten in place
(`writeBuffer` on the geometry's buffer for that stream) without
touching the others. Three's `setDrawRange` (a `DrawRange` type exists
in core) fits here too. Point size and a stock points material are a
separate, smaller item (the survey's list,
okf/notes/three-feature-survey.md).

The engine already has the mechanism: instance attributes carry a
buffer slot, an entry binds one buffer per slot, and the VAO builder
records each slot's layout with its own stride and divisor. Vertex
streams are the same list at divisor 0. With no compatibility to keep,
the pipeline takes WebGPU's shape: one list of buffers, each with a
step mode (`vertex` or `instance`) and its attributes, replacing the
separate `attributes` and `instanceAttributes` lists in `PipelineDesc`,
the flux marshalling, the `flux:gpu` types and core. Three's
one-buffer-per-channel is then a stream per attribute, so no third
shape is needed. Stage 1 must not add anything that assumes one vertex
buffer beyond what exists.

In the 3d package stage 2 touches `Geometry` (a streams form beside
`vertices`, the accessor picking the stream), `geometry-gpu.ts`
acquisition and release, the entry binding in `scene.ts`, and the
picking shape, which needs to know which stream positions live in.
The identity-keyed upload cache in `geometry-gpu.ts` (keyed by the
vertices array) is what in-place updates replace: a stream update
needs an explicit geometry handle, not array identity.

Falls out of stage 1 for free: instance records take packed formats
from the same table.

## Stage 1 landed (2026-09-11)

The vocabulary and the 3d package as written above, verified by the
geometry, glTF, sweep and order rigs, `srt check` on core, 2d and 3d,
and the alloy and flux test suites. Not in this landing: instance
attributes in the 3d package stay float32-family (the record and style
buffers are Float32Arrays written in floats); the packed formats are
reachable for instance records through core's pipeline API. Additive
later: records as byte views.

## Findings

- The engine never matched a pipeline's attribute formats against the
  program's reflection: `program_attributes` is reported to JS and the
  by-name match lives in the 3d package (`missingAttributes`), so the
  component-count rule has exactly one home.
- `srt check <package>` typechecks `examples/`, not `checks/`: a rig
  is runtime-only, and a type change that breaks a rig shows up as a
  failed run, not a failed check.
- `DataView.getFloat16/setFloat16` and `Float16Array` exist in bun and
  in flux's QuickJS, and TypeScript 7 under `lib: ["ESNext"]` knows
  them, so half floats need no conversion helper.
- Three untracked alloy examples (draw_ordered, draw_instanced,
  sub_target) fail to compile on a `create_draw_target` argument-count
  drift that predates this work.
