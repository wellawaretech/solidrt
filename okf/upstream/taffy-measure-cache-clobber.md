---
title: taffy measure cache evicts entries it can still hit
description: Cache::store picks a slot from the input shape alone (9 slots) while Cache::get matches on shape AND parent width, so the same-shape/different-parent-width probes a single flex pass makes evict each other; the cache is defeated frame-internally and one dirty node re-measures the whole tree.
project: taffy (github.com/DioxusLabs/taffy)
versions: taffy 0.12.1
status: unfiled
link:
created: 2026-08-03
---

# taffy: measure cache evicts entries it can still hit

Found 2026-07-17 during the layout-perf design session (the "184 rows =
seconds of layout" problem); written up here 2026-08-03. Full local
history in okf/done/layout-cache.md.

## Draft report

In `src/tree/cache.rs`, the slot a measurement is stored in and the key it
is retrieved by disagree about what identifies a measurement.

`store()` picks the slot from the input SHAPE only:

    let cache_slot = Self::compute_cache_slot(input.known_dimensions, input.available_space);
    self.measure_entries[cache_slot] = Some(CacheEntry { key, content: layout_output.size });

`compute_cache_slot` returns one of 9 values (which dimensions are known x
min-content vs max-content/definite), and the store is unconditional.

`get()` never consults the slot function at all - it scans all entries
linearly and compares a key that also includes the parent width:

    if entry.key.kd_available_space == key.kd_available_space
        && (entry.key.x_axis_parent_size() == key.x_axis_parent_size())

So the eviction policy discriminates on strictly less information than the
hit key. Two probes with the same shape but different parent widths are
distinct entries to `get` and the same slot to `store`: each store evicts
an entry that was still reachable, and afterwards neither key hits.

This is not a rare collision, it is the normal path. A flexbox pass
legitimately probes the same child with the same shape under several parent
sizes (the parent size is unknown during intrinsic sizing and resolved for
the final pass), so the alternation happens within a single layout pass on
ordinary content. The effect is that the measure cache is defeated
frame-internally, not merely across frames, and any single dirty node
re-measures the whole tree.

Measured here before replacing the cache, 1014 nodes with one text node
changed per rebuild: 9907 cache lookups, 6063 misses, 4846 text measures.
With a cache that keeps taffy's hit semantics but does not self-evict:
7 measures, 2659 hits out of 2671 lookups (99.6%). Counts are the evidence;
the two runs are not on the same build profile so their times are not
comparable.

Suggested fix: decouple eviction from the shape. Because `get` already
scans linearly, `compute_cache_slot` is pure eviction policy - `store` can
update in place on an exact key match and otherwise take the next slot in
a ring, which fixes the clobber without changing hit semantics at all.
That is what our replacement does.

Note that raising `CACHE_SIZE` alone does NOT fix it: `compute_cache_slot`
returns 0..8 regardless of the array length, so the extra slots are simply
never written.

## Local impact and workaround

Replaced wholesale: `LayoutCache` in alloy/src/rendertree/layout/cache.rs
(145 lines) behind the `CacheTree` impl we already own
(alloy/src/rendertree/layout/context.rs:72), plus 6 tests in
alloy/src/tests/layout_cache.rs. It mirrors taffy's key semantics exactly,
including matching on parent width only (parent height and the requested
axis are masked out of measure hits), but stores measures in a 16-slot ring
with same-key update-in-place. Bounded at ~0.5 KB per node, roughly 2x
taffy's fixed footprint.

`get_stats` exposes cacheGets/cacheHits permanently, so a low hit rate at
scale is the tell that a cache is being defeated again.

On `resolved`: cache.rs and its tests can come out and the `CacheTree` impl
can go back to `taffy::Cache`. Not urgent even then - the ring is bounded
and behaves well - so this is a maintenance reduction, not a fix we are
waiting on.
