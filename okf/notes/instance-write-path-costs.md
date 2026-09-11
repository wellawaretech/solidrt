---
title: What a per-frame restyle of instance records costs on each JS write path
description: Measured on the release client under QuickJS - setInstanceStyle per instance is about a microsecond of call machinery each, the codec path about two, the accessor half that, and a bulk typed-array loop over the stream mirror the floor; the app's own per-instance math dominates the floor.
created: 2026-09-11
---

# What a per-frame restyle of instance records costs on each JS write path

Measured with probes/3d-restyle-bench.tsx on 2026-09-11 (laptop, Intel
RPL-P, release client, 60 fps): two instanced meshes of N boxes each,
every instance restyled every frame with a fresh sin/cos tint. `floats`
is a custom class with a `float32x4` style record (the all-float fast
path), `halves` the stock `unlit({ instanceColors: true })` with its
`float16x4` record (the codec path). JS milliseconds per frame of the
restyle loop, averaged over 60 frames, one mesh at a time:

| path | N = 1000 floats / halves | N = 10000 floats / halves |
| --- | --- | --- |
| `setInstanceStyle` per instance | 3.8 / 5.0 | 11.4 / 18.7 |
| `instanceAttribute` set per component + one `updateRecords` | 3.7 / 3.4 | 9.0 / 9.3 |
| typed view over `stream.data`, indexed loop, one `updateRecords` | 1.9 / 2.2 | 5.2 / 5.3 |

Reading the table:

- The bulk row is the floor, and most of it is the tint computation
  itself (a `Math.sin`, a `Math.cos` and a four-element array per
  instance): the write is four indexed stores. So a population restyled
  per frame costs what its own per-instance math costs, plus nothing.
- The accessor adds about 0.4 us per instance for four component
  calls: a closure, a codec call and a DataView store each. Cheaper than
  expected; the format makes no difference (a half float store is a
  DataView call like a float store).
- `setInstanceStyle` adds about 0.6 us per call over the accessor on
  the float path and 1.3 us on the codec path: the per-call machinery
  (the disposed check, the length check, the dirty-range extend and the
  scene hook) plus, for a packed layout, the codec pass. An indexed
  store loop beats `TypedArray.set` over a plain array under QuickJS,
  which walks the array-like generically (10 percent on this path).
- None of this is a memory story: the mirror copy in `setRecords` is a
  native memcpy and the deferred publish is one `writeBuffer` per
  stream per frame however many records changed.

The rule this sets, in packages/3d/AGENTS.md: the sugar is for few
records, the mirror is for many.
