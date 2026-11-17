---
title: A layout move keeps a boundary's own cache
description: A boundary whose box is only moved by someone else's relayout (a sibling above it grew, a card was inserted above a list) loses its own cache, because the layout pass clears from the moved node up; a snapshot card in a list re-rasterizes for every insert above it, though its cached content does not depend on where it is. A write to the node's own left/top is not this case and still clears its cache - layout props are not for animation.
created: 2026-11-17
---

# A layout move keeps a boundary's own cache

## Problem

`set_unrounded_layout` (`alloy/src/rendertree/layout/context.rs`) clears
the paint caches from the node up whenever the node's computed layout
changed. The clear starts at the node itself, also when only the
location changed. So a boundary that is moved by a relayout loses its
own cache: a snapshot is marked stale and re-rasterized (a blocking RPC
to sol-raster), and a recording is dropped and recorded again.

The cached content does not depend on the location. A boundary's origin
is on a whole device pixel wherever it is (`composite::snapped_own`),
and the recording or the texture is fixed relative to that origin. Only
the ancestors hold the old place.

Cases: a list of snapshot cards with an insert above them, a panel
pushed down by a sibling that grew.

Not a case: a node moved by a write to its own `left`/`top`. These are
layout props, and layout props are not for animation: a node that moves
every frame moves by `x`/`y`. The write reports `Damage::Layout`, which
clears the node's own cache before the layout pass runs, and that stays
as it is.

Found by reading, while reviewing
[boundary-cache-under-js-written-scale](../plans/boundary-cache-under-js-written-scale.md).
Confirm it with the paint counters before the fix.

## Proposed fix

When the location changed and the size did not, start the clear at the
parent. A size change clears from the node, as today.

The snapped box of a moved node can differ by one device pixel (the far
edge rounds against another fraction). A snapshot's key already catches
that: the box does not match, and it re-rasterizes once. A recording
takes its clip and its box from the walk at composite time.

## Done looks like

- A snapshot boundary moved by a relayout, with the same size, is reused
  (`snapshotsReused`), unless its snapped box changed.
- A Recording boundary moved by a relayout, with the same size, is
  replayed (`boundariesReused`, not recorded).
- The ancestors' caches are cleared, and the node moves on screen. The
  damage holds its old and its new extent.
- A size change clears the node's own cache, as today.
- Tests: a lib test for the recording half, and a case in the GL example
  of boundary-cache-under-js-written-scale for the snapshot half.

## What was built (2026-11-17)

Confirmed with the counters first: a Recording card pushed down by a
sibling was recorded again, a Snapshot card was re-rendered, and an
insert above four snapshot cards re-rendered all four. After the change
they are replayed and reused.

- `set_unrounded_layout` starts the clear at the parent when the layout
  changed only in where the box sits (`only_placed`: `location`, `order`
  and `margin` may differ; `size`, `border`, `padding`, `scrollbar_size`
  and `scrollable_overflow_rect` are equal). The node's envelope is
  stated in its own slot frame, so it stays valid too.
- A Recording boundary that is not a View keeps the old behaviour: its
  recording holds its box offset, a fraction of a device pixel that
  depends on where it sits. JS cannot make one.
- A recording now holds the box it was recorded at
  (`RecordingCache::size`). At the density it was recorded at, another
  box re-records it: the far edge of a moved box can round to another
  device pixel. Under another density the boxes are not compared, so a
  recording still replays under a moving ancestor scale.
- Found on the way and fixed: on the frame a clipping Recording boundary
  recorded, it clipped at its last child's box. The record path read
  `ctx.size` after the record walk, whose child loop leaves it at a
  child's box.

Known and not changed: a child that moves changes its parent's
`scrollable_overflow_rect`, so the parent's layout changes too and its
whole extent joins the frame damage. The moved node keeps its cache, but
the repaint covers the parent.

Tests: `alloy/src/tests/boundary_move.rs`, the clip test in
`alloy/src/tests/composite.rs`, and the GL example
`alloy/examples/boundary_layout_move.rs`.

## Involves

`set_unrounded_layout` in `alloy/src/rendertree/layout/context.rs`.
