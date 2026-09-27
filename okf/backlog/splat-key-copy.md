---
title: The engine holds a full 32-byte record copy of a splat cloud when the sort needs 12
description: An indexed instance order keys on the record's position, but the hand-off mirror the core sorts is the whole record block (32 bytes per splat, 32 MB at 1M) next to the same bytes in the record texture; a position-only key copy would cut the CPU-side residency to 12 bytes per splat, and the memory pressure that reaped the phone client at 1M was real.
created: 2026-09-27
---

# The engine holds a full 32-byte record copy of a splat cloud when the sort needs 12

## Symptom

okf/notes/gaussian-splats-against-the-field.md: `transferRecords` hands
the full record block to the core as the sort's key source, and the
indexed order reads only the 12-byte position per record. At 1M splats
that is 32 MB where 12 would do, on top of the 32 MB texture and the
4 MB id stream.

## Shape

The order's `records` layout already says where the position is; the
hand-off (or the indexed order) could take a position-only key stream
(float32x3, 12 bytes) instead of the record, with the same mirror
lifetime. The bake could even write the positions as their own block
so the copy is a view. Additive on InstanceOrder and createRecordMesh.
