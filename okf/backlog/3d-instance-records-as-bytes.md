---
title: The 3d package's instance records are float32 only, so packed instance formats stop at the engine
description: A shaderMaterialClass instanceBuffers layout must be float32-family and tightly packed because the mesh writes its record and style buffers as Float32Arrays counted in floats, while the engine accepts every vertex format and an explicit stride on an instance-step layout.
created: 2026-09-11
---

# The 3d package's instance records are float32 only, so packed instance formats stop at the engine

## Symptom

`shaderMaterialClass({ instanceBuffers })` (packages/3d/src/material.ts)
rejects a non-float32 format, an `arrayStride` and an attribute
`offset`, because `instanceStride` (mesh.ts) counts floats and the
record, style and record-mesh buffers are Float32Arrays written by
float index. A per-instance color is 16 bytes where a `unorm8x4` is 4,
and a record mesh cannot carry a packed record. The engine side has no
such limit (any layout on a `stepMode: "instance"` buffer; see
okf/done/3d-vertex-data-model.md).

## Comparison

Three's `InstancedBufferAttribute` takes any typed array with
`normalized`; Godot's MultiMesh packs its per-instance color and custom
data as 8-bit or half formats.

## Done looks like

Instance layouts take the full vertex vocabulary and strides in bytes;
records are byte views written through the attribute accessor
(`attributeAccess` over the record buffer, as vertex streams are), and
`setInstanceStyle`/`setRecords` accept floats and encode on write.
The core-written matrix record (INSTANCE_MATRIX_ATTRIBUTES) stays
float32x4 by the spatial core's contract.

## What it involves

`packages/3d/src/mesh.ts` (strides, the Float32Array records and style
mirror, the record mesh), `material.ts` (the validation), the style
publish path in scene.ts, and the AGENTS.md instanced-material section.
