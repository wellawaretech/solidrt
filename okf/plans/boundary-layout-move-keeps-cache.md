---
title: A layout move keeps a boundary's own cache
description: A boundary whose box only moves (a sibling above it grew, a drag by left/top) loses its own cache, because the layout pass clears from the moved node up; a snapshot card in a list re-rasterizes for every insert above it, though its cached content does not depend on where it is.
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

Cases: a list of snapshot cards with an insert above them, a card
dragged by `left`/`top`, a panel pushed down by a sibling that grew.

Found by reading, while reviewing
[boundary-cache-under-js-written-scale](boundary-cache-under-js-written-scale.md).
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

## Involves

`set_unrounded_layout` in `alloy/src/rendertree/layout/context.rs`.
