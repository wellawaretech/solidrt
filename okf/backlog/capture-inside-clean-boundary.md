---
title: captureSnapshot fails inside a clean repaint boundary
description: A capture (captureSnapshot, /snapshot) of a node under a repaintBoundary view whose recording is being reused fails with "capture node is not in the live render tree", because the Recording branch replays the cached display list without descending, so the paint walk that services captures never reaches the node.
created: 2026-08-27
---

# captureSnapshot fails inside a clean repaint boundary

What it looks like: `/snapshot?node=N` of a `d-path` inside one of the
parse-svg example's `repaintBoundary` views answers `capture node is not in
the live render tree` while `/tree` lists the node with a box. Dirty the
boundary (hover a shape; or pause the clock, send a pointer move, step) and
the same capture succeeds.

Why: captures are serviced by the paint walk reaching the node
(`composite.rs`, `build_recursive`). Culling already defers to pending
captures (`has_pending_captures()` skips the envelope test, since a capture
target may be off screen), but `BoundaryMode::Recording` replays its cached
recording and returns before descending, so a node inside a clean boundary
is never visited and `fail_unserviced_captures` reports it as not live.
Snapshot boundaries (`snapshot_node`) reuse their texture the same way and
presumably fail the same way.

Fix shape: give the boundary reuse the same exemption as culling. While
captures are pending, walk into the boundary's subtree instead of replaying
(re-recording is the simplest; a capture is rare, so the extra recording is
free in practice). The point is that "in the tree" and "visited by the walk"
agree, whatever the boundary's cache state.
