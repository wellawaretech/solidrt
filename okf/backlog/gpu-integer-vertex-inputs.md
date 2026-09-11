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
