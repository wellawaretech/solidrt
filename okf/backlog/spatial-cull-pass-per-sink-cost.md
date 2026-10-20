---
title: The cull pass costs microseconds per sink on a phone core
description: A camera move re-tests every draw sink on the target, and on the tablet that is 5.6 to 6.6 ms for 1600 sinks, about 3.5 us each, for a frustum test and a LOD check; the per-sink hash lookups and box walks are the suspects, and a profile with a per-target index or a faster hasher should cut it well below the occlusion work that now sits beside it.
created: 2026-10-20
---

# The cull pass costs microseconds per sink on a phone core

## Symptom

`probes/3d-occlusion-probe.tsx` with its occluders off (the wall drawn,
not occluding): 1600 solids in view, the camera moving every frame. On
the tablet (SM-T500) the scene sync, which is the flush with the cull
pass inside, is 5.6 to 6.6 ms at steady state, about 3.5 us per sink,
with the occlusion pass contributing nothing (no occluder shown, nothing
built, nothing tested). On this machine the same walk is 0.7 to 1.4 ms.
The occlusion gate added beside it measured within the noise at 128
texels wide; the frustum re-test is the bulk, and it is what every
camera move pays on the low row, with or without occluders.

## Suspects

Per sink, `cull_pass` (`alloy/src/spatial/mod.rs`) does several
`HashMap<u64, _>` lookups keyed by target with the default SipHash
(`frustums`, `lod_views` through `lod_allows`, `occlusion`,
`draw_sorts`), takes `cull_box` (a group union walk for instanced and
skinned meshes), tests six planes, and runs the LOD check. A hundred
nanoseconds per lookup on a little core adds up to the figure seen.

## Done looks like

- A profile of the pass on the tablet (perf, or a counter per step) that
  says where the 3.5 us go.
- The fix that follows: a per-target index resolved once per pass
  instead of a hash lookup per sink (the frustum, the LOD view, the
  occlusion buffer and the sort state as one struct looked up once per
  target, sinks grouped by target), or at least a faster hasher for the
  u64 keys (FxHash-class, already a workspace dependency or an easy pin).
- The same probe's baseline well under half of today's, the figure in
  this note's findings when done.

## Not in this item

The occludee test's own cost (eight corner projections and a quadtree
walk per sink), which did not show above the noise; a SIMD projection
is the lever if a population ever asks, noted in
[3d-occlusion-culling](../done/3d-occlusion-culling.md).
