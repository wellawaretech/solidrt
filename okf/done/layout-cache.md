---
title: Layout cache replacement
description: Taffy's one-entry-per-shape node cache self-clobbers within a flex pass, re-measuring the whole tree for a one-node change; replaced by a 16-slot ring keeping taffy's key semantics.
created: 2026-07-17
completed: 2026-07-17
---

# Layout cache replacement

Outcome of the layout-perf design session (the postmortem's "184 rows =
seconds of layout" problem). Implemented and verified 2026-07-17.

## Root cause

Taffy's per-node cache (`taffy::Cache`) keeps exactly one entry per input
shape (9 shapes: which dimensions are known x min/max-content), but the hit
key also includes the parent width. One flex pass legitimately probes the
same child with the same shape under several parent sizes (unknown during
intrinsic sizing, resolved later), so the slot is written with alternating
keys within a single frame. Each store clobbers the previous entry; nothing
survives even frame-internally, and a one-node change re-measures the whole
tree. Measured before the fix (1014 nodes, one text changed per rebuild):
9907 cache lookups, 6063 misses, 4846 text measures, layoutMs 41.8 (debug
build; counts are the evidence, ms inflated).

## Decision

Replace `taffy::Cache` with our own `LayoutCache`
(alloy/src/rendertree/layout/cache.rs), behind the `CacheTree` trait we
already implement - no taffy fork. Identical hit-key semantics (per-axis
known/available bits + parent width for ComputeSize; full input for
PerformLayout), but measure entries live in a 16-slot ring with same-key
update-in-place, so all parent-size variants of a pass stay live. Bounded:
~0.5 KB per node, ~2x taffy's fixed footprint, no growth over time.

Considered and rejected for now: dropping parent_size from the key for
nodes with no percentage styles (pct() exists, more reasoning surface, and
capacity alone ends the thrash); enlarging CACHE_SIZE upstream (fork).

## Results (same 1014-node scenario, release client)

- measureCalls 4846 -> 7, cacheHits 2659/2671 (99.6%)
- layoutMs 0.87 (release; was 41.8 on a debug client before the fix)
- paraShapes/dirtiedNodes unchanged (3/3) - shaping was never the problem
- rendering pixel-verified; alloy tests green incl. 6 new LayoutCache tests

The remaining ~2600 lookups per rebuild are the dirty container re-probing
its children, all answered from cache; cost is one arithmetic pass, scales
with the dirty container's child count, not tree size.

## Futures

- Report the clobber pattern upstream to taffy (same-shape probes under
  alternating parent sizes defeat the one-entry-per-shape cache).
- get_stats exposes cacheGets/cacheHits permanently; a low hit rate at
  scale is the tell that this cache is being defeated again.
- The postmortem's 3.3 items 2-4 (cause-attribution ring buffer, span
  profiler, scaling harness) stay parked; the counters answered the
  question without them.