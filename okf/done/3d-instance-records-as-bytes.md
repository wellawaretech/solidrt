---
title: The 3d package's instance records are float32 only, so packed instance formats stop at the engine
description: A shaderMaterialClass instanceBuffers layout must be float32-family and tightly packed because the mesh writes its record and style buffers as Float32Arrays counted in floats, while the engine accepts every vertex format and an explicit stride on an instance-step layout.
created: 2026-09-11
completed: 2026-09-11
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

## Shape: an instance buffer is a vertex stream stepped per instance

The least-effort fix would have bolted the formats onto the two paths as
they were - a style record with a mirror, a dirty range and per-write
encoding beside a record mesh with no mirror, no partial update and
app-side encoding. That asymmetry was an accident of when each was
written, so the item took the unified shape instead: the model the
package already has for vertex data (`Geometry.streams`,
`attributeAccess`, `updateVertices`) applied one step up.

- `InstanceStream` (mesh.ts): the material's layout for the buffer, a
  byte stride, a mesh-owned GPU buffer, a byte mirror (`vertexView`'s
  rule: a Float32Array over an all-float layout, a Uint8Array otherwise),
  the encoded blank record, a dirty byte range. `MeshInstances` is the
  population bookkeeping plus `matrix` (the core-written buffer of an
  instanced mesh, null on a record mesh) and `streams` (every other
  declared instance buffer, in order). The two-slot cap is gone.
- `instanceAttribute(mesh, name)` is `geometryAttribute` for records:
  the accessor reads through the stream's current view, so it survives
  growth (a held `data` view does not). `updateRecords(mesh, { stream?,
  first?, count? })` is `updateVertices`: marks the range, the scene
  publishes one buffer write per dirty stream at sync. `setInstanceStyle`
  (values per component, encoded through the codecs) and `setRecords`
  (bytes in the layout, whole rewrite, grows) stay as the sugar.
- Pairing and binding go by layout key (names AND formats), not stride:
  a byte-equal layout in other formats would decode differently. A
  material's declared buffers each bind the matrix or the stream with
  that key, in the material's order, so a shadow variant that reads a
  subset binds the right buffers.
- Dirty ranges persist across detach; attach republishes whatever is
  pending instead of the whole mirror. Growth marks the old capacity.
- Tight packing stays (explicit `arrayStride`/`offset` still throw, as
  geometry layouts have no padding either); the matrix record stays
  float32x4 by the core's contract.

Stage 2, decided with it: the stock `instanceColors` record is
`float16x4` (8 bytes per instance, no banding in the darks that a linear
8-bit unorm would show), and `packages/3d/examples/instanced.tsx` carries
its tint as `unorm8x4` built through the accessors (a 28-byte record
became 20).

## Landed (2026-09-11)

Verified by `srt check` on the 3d package, probes/3d-packed-records-probe.tsx
(unorm8x4 + float16x2 style records through setInstanceStyle and by name,
growth, the blank, a packed record mesh built through the accessor, a
partial rewrite; mirrors checked in JS, GPU buffers read back byte for
byte through `/buffer`), the ported probes/3d-instance-probe.tsx
(growth, recycle blank, style readback, pick, tap bubbling) and
probes/instanced-grow-probe.tsx (setRecords growth to 2000 while
attached), and the instanced and fleet examples on a window.

## Findings

Cut into okf/notes/instance-write-path-costs.md: the measured per-frame
cost of each record write path under QuickJS, which set the AGENTS.md
rule that the per-instance sugar is for few records and the mirror is
for many.
