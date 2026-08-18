---
title: Paint viewport culling
description: The paint walk visits and builds every mounted node whether or not it can be seen, so paint cost is O(mounted content) - ~7 us/node, ~155 ms/frame at 17k nodes; add a cull rect to the walk and a conservative per-subtree paint envelope so off-screen subtrees are skipped before build().
created: 2026-08-18
---

# Paint viewport culling

## Symptom

A long scrollable document (one `overflow="hidden"` scroller, every block
mounted, no windowing) paints at O(total mounted content), not O(visible
content): with the same ~1300 px viewport, paintMs goes 24 -> 40 -> 155 ms
as the mounted tree grows 900 -> 4.5k -> 17.8k nodes. Layout is not
involved (layoutMs ~0, layout cache near 100% hits, no reshaping under
scroll). The whole cost is the paint walk.

## Cause

`record_node` (`alloy/src/rendertree/composite.rs`) descends into every
child and calls `build()` on it. There is no rect test on the paint side;
the only rect gate in the rendertree is the hit-test overflow gate in
`hit.rs`. Impeller's display-list builder does drop ops whose bounds miss
the current clip, which is why the raster side is not also drowning - but
by then the traversal, the text `draw_paragraph` calls (one per placed
word, each building a text frame before Impeller rejects it) and the
translate/save ops have all been paid on the main thread.

## Design

Two pieces, no API surface.

**Cull rect in `BuildContext`.** `Option<Rect>` in the CURRENT local
space; `None` means "unknown, cull nothing". Root: the window box. Into a
child: translate by `-location`. At an overflow-clipping node: intersect
with the box per clipped axis (mirroring `apply_clip`), then translate by
`+scroll`, then map through the inverse viewBox fit. At a node with an own
matrix: bounding box of the rect through the inverse (a superset, still
conservative); non-affine or singular -> `None` for the subtree. Same
order as the record order (matrix, clip, scroll, fit) so it cannot drift
from what is drawn.

Reset to `None` inside every repaint-boundary recording (Recording and
Snapshot) and inside `service_captures`: those caches survive an
ancestor's scroll (`invalidate_paint` walks up, not down), so content
recorded against a viewport-culled subset would replay wrong at the next
offset, and a capture must contain everything. The boundary NODE may
still be skipped by its parent; only its cached content is recorded
uncalled.

**Subtree paint envelope per element.** `Cell<Option<Rect>>`, the union
of the node's own painted box and its children's envelopes (each at its
location, through its own matrix's bounding box), in the node's own local
space, pre-own-matrix. Conservative rules:

- a node that clips an axis contributes only its box on that axis
  (children cannot paint outside it) - this makes a scroller O(1);
- a node whose extent is not reliably known (a detached text without
  explicit `w`/`h`, any kind without a trustworthy `local_bounds`) is
  UNBOUNDED, and unboundedness propagates up to the nearest clipping
  ancestor; unbounded is never culled;
- text with layout: layout box, union the owned layout's line extent when
  it overflows the box, plus a small ink outset.

Invalidation rides on `invalidate_paint`, already the universal upward
walk for every Paint/Layout/Compose damage and for every changed taffy
layout (`set_unrounded_layout`); the envelope has exactly the validity
conditions of a boundary recording. Recompute is lazy in the paint walk,
so a scroll costs depth plus siblings, not O(n).

The child loop then skips a child whose envelope, in the parent's space,
does not intersect the cull rect. Anything uncertain maps to `None` /
unbounded, never to a wrong skip.

## Stages

1. Cull propagation through translate / clip / scroll only; matrix, fit,
   boundaries, captures reset to `None`. Envelopes as above. A
   `nodesPainted` per-frame counter in `PaintStats` so the effect is
   visible in `get_stats`.
2. Inverse-map the cull through affine own matrices and viewBox fits.
3. (Maybe never) a Recording boundary remembers the cull it was recorded
   with and re-records when the new cull is not a subset - only if a real
   app needs a boundary inside a long scroller.

## Findings

Landed 2026-08-18: `alloy/src/rendertree/cull.rs` (envelope + cull-rect
steps), wired in `composite.rs`; `nodesPainted` in the stats. Measured on
the same 1989-block, 17,841-node document, no repaint boundary, release
client, Intel iGPU:

- nodesPainted 54 at the top, 29 mid-document, 33 at the end (of 17,841).
- paintMs 154-161 -> 5-7 (13.5 on the mount frame, the first envelope pass).
- Memory flat under 80 page scrolls; a reload settles at 54 painted at
  once. Layout untouched (layoutMs 0.01 under scroll).
- Visual check by scrolling: nothing missing or late at the viewport
  edges.

Remaining floor: 5-7 ms for ~50 nodes is the envelope test over the
scroller's ~2000 direct children each frame - O(blocks), not O(visible).
Cheap enough; if it ever matters, skip ahead once a flex column's children
pass the cull rect (positions are monotone along the main axis).

Trap: a Recording boundary around long content masks this entirely (its
recording is replayed, the walk never enters), so measure without one.

## Related

The same document retains ~800 B per character: each text's `OwnedCache`
holds an Impeller `Paragraph` per word for the node's lifetime, and the
shared word LRU (8192) does not cover a 200k-word document, so nearly
every word ends up with its own paragraph object (~4 KB). The fix -
retain `RunMetrics` only and shape on paint through the word cache -
depends on this item: without culling, shape-on-paint reshapes the whole
document every frame. Separate item, after this one.

Today's mitigation without engine work: a Recording repaint boundary on
the scroller (`Hoist::Full` reuses the recording across scroll writes).
