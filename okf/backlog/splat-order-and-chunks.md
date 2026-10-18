---
title: The splat order block and chunk table carry less than they could - the count dial drops the subject, sparse leaves lose precision, and nothing culls a chunk
description: The order block ranks by volume times opacity, so a capture whose subject is micro-splats loses the subject before the background at 100k; a k-d leaf holds 256 splats whatever their spread, so 0.9% of the lizard's splats in near-empty space move more than a tenth of their size; and the chunk table the core now holds is never used to skip off-screen chunks in the sort, the one lever left on the phone's ~44 ms fill at 1M. Three jobs on the same two structures, decided together.
created: 2026-10-18
---

# The splat order block and chunk table carry less than they could

## Symptom

From okf/done/splat-record-compression.md and okf/design/splats.md:

- **The count dial.** `scratch` renders of the lizard at 786k / 300k /
  100k (2026-10-18): at 100k the garden, path and plinth survive as
  coarse shapes and the lizard dissolves into a smear, because
  importance = volume x opacity ranks the big background splats first
  and the subject is all micro-splats. The prefix is the only LOD dial.
- **Sparse leaves.** The bake report on the lizard: position step over
  the splat's largest axis p99 7.7%, 0.9% of splats above a tenth, all
  in chunks of near-empty space (the top-left and the trees); a leaf is
  256 splats whatever their spread, so the box is wide there.
- **Culling.** The Pixel 7 at 1M full resolution spends ~44 ms in fill
  (display blend) and ~8 in the vertex side; off-screen splats are
  culled per vertex but still sorted, materialized and fetched, and
  nothing on screen is drawn less. The core now holds every chunk's box
  (the mirror's `bounds`) and could skip whole chunks before the key
  pass with a frustum instead of a direction.

## Shape

- **Ranking at bake.** Keep importance as the base and interleave
  spatial coverage so a prefix is a thinning everywhere rather than the
  big splats only: the bit-reversed k-d leaf order within importance
  bands of the ORDER block (not of the storage order, which stays
  spatial), or a rank by screen contribution at a reference distance
  (projected area x opacity at the capture's median camera distance,
  which the header could carry). Judge on the lizard at 100k and 300k
  against the plain importance prefix; the train must not get worse.
- **Sparse leaves.** Split a leaf further while its box step exceeds
  `SPLAT_STEP_VISIBLE` of its smallest largest-axis and pad the short
  leaves to 256 with records the order block never lists (never drawn,
  never sorted), under a padding budget; or split at the largest spatial
  gap along the longest axis when the gap dominates the extent. The
  core's `check_packed` already tolerates an order shorter than the
  records once relaxed to "each id once".
- **Chunk culling in the sort.** The scene feeds a frustum (six planes
  in the mesh's frame) beside the direction; the registry's key pass
  tests each chunk's box once (4k boxes at 1M) and skips its records,
  and the published population is what passed. Needs the feed through
  `SinkWriter`, a per-chunk visibility pass before the radix sort, and
  the count prefix to mean "of the visible". Measured on the Pixel 7
  ladder: fill at 1M full res with the camera inside the train's
  volume, and the sort ms.

Three jobs, one session each in that order, since the second changes
what the first ranks and the third changes what both publish.
