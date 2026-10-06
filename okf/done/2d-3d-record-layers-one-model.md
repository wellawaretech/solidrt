---
title: The 2d records layer and the 3d record mesh publish raw records two different ways
description: createRecordLayer owns the Float32Array and publishes it in place through the write lease (records, withRecords, touch), createRecordMesh copies app-owned records in and republishes ranges (setRecords, updateRecords, setRecordCount, instanceAttribute); the same escape hatch, two verb sets, settled on neither side against the other.
created: 2026-10-06
completed: 2026-10-06
---

# Two publish models for raw records

## Symptom

Both packages have a raw-records escape hatch for motion only JS can
compute at scale, and the two were built apart:

| | `@solidrt/2d` `createRecordLayer` | `@solidrt/3d` `createRecordMesh` |
| --- | --- | --- |
| who owns the array | the layer (`layer.records`, capacity-sized) | the app (`records` passed in, copied into a mirror) |
| write | in place, then `touch()` (or `withRecords(fn)`) | `setRecords(mesh, records, count?)` rewrites from the start |
| partial write | none (the live prefix republishes whole) | `instanceAttribute` + `updateRecords(mesh, { first, count })` |
| population dial | `addSprite`/`destroySprite` shift records | `setRecordCount(mesh, n)` |
| layout | one fixed 16-float record (`INSTANCE_FLOATS`) | the material's instance layouts, any vertex format, several streams |
| growth | doubles the layer's array and buffer | doubles the mirror into replacement buffers |

The 2d model is the zero-copy lease (measured 2.4x faster than setSprite
at 30k sprites, [2d-spatial-citizenship](../done/2d-spatial-citizenship.md));
the 3d model is the vertex-stream shape
([3d-instance-records-as-bytes](../done/3d-instance-records-as-bytes.md)),
which unified 3d's own two paths and did not look across. A porter
writing a particle system for both learns two APIs for one idea, and
the 2d side has no partial write where the 3d side has no in-place one.

## Shape

One verb set over both, the engine's cost model deciding which: the
lease-based in-place write where the whole population moves every frame
(the 2d case, and a 3d particle sim), the copy-in range write where a
few records change. Candidates: `records`/`touch()` on the 3d record
mesh beside `setRecords`, or `updateRecords({ first, count })` on the 2d
records layer beside `touch()`, with the constant named the same on both
sides (`INSTANCE_FLOATS` since 2026-10-06). The Three/Unity/Godot
comparison for the proposal: Three's `InstancedBufferAttribute` is the
app-owned array with `needsUpdate` and `addUpdateRange`, Unity's
`GraphicsBuffer.SetData` is copy-in with a range, Godot's `MultiMesh`
`set_buffer` is copy-in whole. None owns the array for the app, which
argues for keeping the 2d lease as the fast path and adding the range
write, not for removing the lease.

## Involves

`packages/2d/src/records.ts`, `packages/3d/src/mesh.ts` (the record
mesh half), both AGENTS.md records sections, `examples/instanced.tsx`
and the 2d demos that write records raw.

## Outcome (2026-10-06)

One verb set over the same algorithm on both sides, implemented in each
package (the core extraction is filed in tiny.md as the mechanical move
it now is):

- `records(x, stream?)` is the mirror on both: a Float32Array over an
  all-float layout, bytes otherwise, read at use time because growth
  replaces it. The 3d one is new (the mirror was reachable only through
  `_instances.streams[i].data`); the 2d `layer.records` field and the
  unused `withRecords` are gone.
- `updateRecords(x, { first?, count? })` publishes a range on both. The
  2d flush now tracks a dirty record range: a plain layer publishes it
  as one `writeBuffer` at its byte offset (clipped to the live sprites),
  an `orderBy` layer the drawn prefix whole through the lease, growth a
  replacement buffer filled whole - the 3d `publishRecords` algorithm
  line for line. The sprite verbs mark their own record, so a
  `setSprite` on a records layer publishes one record. `touch()` is gone.
- `setRecordCount(x, n)` dials the drawn prefix on both. On the 2d
  layer the first n live sprites draw and pick, the rest stay live but
  undrawn and unpicked, and the dial outlives adds and destroys (write
  ahead, dial after); `layer.count` stays the live total. The relay
  demo's pulses use it instead of zeroing shrunk slots.
- `setRecords` stays the mesh's: a record mesh's population is opaque
  data, a record layer's is its sprite handles, so the 2d copy-in is
  `records(layer).set(src)` then `updateRecords(layer)`.

The three 2d demos moved to the new verbs (and their stale
`createRecordLayer(atlas.texture)` calls to the atlas list). Found by
the new test on the way: both layers' `addSprite` spread the options
over the defaults, so an explicit `tint: undefined` left a record's tint
at zero (invisible) where every other options bag in the codebase treats
undefined as absent; both now default with `??`.
`packages/2d/tests/records.test.tsx` and
`packages/3d/tests/records.test.tsx` pin the range, the dial, the
dead-copy rule after growth and the ordered gather on painted frames.
