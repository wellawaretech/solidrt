---
title: A vertex stage cannot declare an integer input, so joint indices ride as floats
description: Program reflection rejects an integer or unsigned `in` at link, every vertex format feeds a float `in` through the pointer's conversion, and the 32-bit integer formats do not exist; a skinned vertex reads `in vec4 aJoints` and casts, and an id or index channel past 2^24 has no exact home.
created: 2026-09-11
---

# A vertex stage cannot declare an integer input, so joint indices ride as floats

## Symptom

`AttrFormat::from_gl` (alloy/src/gpu/vocab.rs) returns None for an
integer or unsigned vector, so linking a program with `in ivec4 aJoints`
throws; every format in the table feeds a FLOAT `in`, integers included
(`uint8x4` arrives as exact integers in a vec4). That was stage 1's
deliberate boundary (okf/done/3d-vertex-data-model.md). It costs a cast
in every skinning stage and rules out any channel that needs more than
24 exact bits, such as a per-vertex id or an index into a wide table.

## Comparison

WebGPU's `uint32`/`sint32` families feed `u32`/`i32` shader inputs;
Vulkan and Metal likewise pair integer formats with integer attributes.
GL ES 3.0 has the mechanism: `glVertexAttribIPointer` for integer
inputs beside `glVertexAttribPointer` for float ones.

## Done looks like

The format table gains a kind column (float or integer); `from_gl`
accepts `INT_VEC*`/`UNSIGNED_INT_VEC*` and the pipeline check matches
kind as well as component count; the VAO builder uses the integer
pointer for integer inputs; the `uint32`, `sint32` (x1..x4) and the
unnormalized `sint8`/`sint16` rows join the table. All additive on the
stage 1 table.

## What it involves

`alloy/src/gpu/vocab.rs` (table, `from_gl`), `alloy/src/gl/program.rs`
(the match), `alloy/src/gl/entry.rs` (`record_layout`), the flux-types
union and the 3d `VERTEX_FORMATS` codecs for the new rows; the skinned
stock stages may then declare `in uvec4 aJoints`.

## Decision: the format decides (2026-09-11)

WebGPU's rule verbatim, no additive carve-out. Every row of the format
table has one kind: float and normalized rows feed a float `in`
(`vec*`) through the converting pointer, `uint*` rows feed `uvec*` and
`sint*` rows feed `ivec*` through the integer pointer. A format never
crosses kinds, so `uint8x4` into `in vec4 aJoints` is now refused at
pipeline creation (and at add() in the 3d package). The alternative,
letting the shader `in` choose the pointer with float inputs still
accepting unnormalized integers, existed only to keep stage 1's
`uint8x4 -> in vec4` working; without that constraint the format-decides
rule is one column, one comparison, and one pointer branch keyed off the
row, and it is what Three/WebGPU, Vulkan and Metal all do.

## Landed (2026-09-11)

- alloy/src/gpu/vocab.rs: `AttrKind` (Float/Uint/Sint) as a table
  column; the `uint32` x1..x4, `sint32` x1..x4, `sint8x4`, `sint16x2/x4`
  rows (26 rows); `from_gl` accepts the integer scalar and vector
  types, reflecting `uvec4` as uint32x4 and `ivec4` as sint32x4;
  `AttrFormat::feeds` is the pipeline rule (components, then kind),
  unit-tested in alloy/src/tests/gpu_validate.rs. The table sits under
  `#[rustfmt::skip]` so it stays one row per line.
- alloy/src/gl/entry.rs `record_layout`: integer rows go through
  `vertex_attrib_pointer_i32`, the rest through the float pointer.
- packages/flux-types/gui/gpu.d.ts: the `VertexFormat` union and the
  `programAttributes` contract (the reflected format is the 32-bit form
  of the input's family).
- packages/3d: `FormatKind` on every codec, the new codecs (a signed
  `sintCodec`, 32-bit int kinds), `formatFeeds` mirroring the engine
  rule and used by `missingAttributes`; the "skinned" preset's joints
  are `uint8x4` (an exporter's common form; u16 joints land in list
  form); `SKIN_DECLS` declares `in uvec4 aJoints` and `boneAt(uint)`;
  the glTF loader keeps a file's u8/u16 joints and narrows an off-spec
  float JOINTS_0 to uint16x4; `.srtm` is version 7 (a version-6 skinned
  part would decode at the wrong stride).
- Verified live (probes/3d-integer-inputs-probe.tsx): a `uint32` id
  channel of odd values past 2^24 read through `in uint` renders the
  exact-id color (a float input could not hold an odd value there), and
  skeleton.glb (u16 joints, float weights) draws posed and lit under the
  stock skinned material.

Honest note: `boneAt` still converts the joint to `int` for
`texelFetch`'s `ivec2`; the cast the symptom mentions shrank from a
float truncation to an integer conversion rather than disappearing.

## Findings

- A skinned model's `bounds` was the bind-pose vertex box in model
  space, before the skin places it: skeleton.glb's armature carries a
  ~100x scale, so a camera framed by `model.bounds` sat inside a foot.
  Fixed in the same pass: the loader folds each joint's box through the
  joint's rest-pose world transform instead (a vertex blended between
  joints lands inside the union's box), which is where the skin renders
  the part at rest.
