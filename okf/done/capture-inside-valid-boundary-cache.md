---
title: captureSnapshot fails for nodes inside a valid boundary cache
description: A capture of a node nested under a repaint boundary whose cache was valid errored with "capture node is not in the live render tree", because the paint walk early-returned at the cached boundary and never reached the node.
created: 2026-09-02
completed: 2026-09-02
---

# captureSnapshot fails for nodes inside a valid boundary cache

## Symptom

Against a static app whose subtree sat under `repaintBoundary` (recording
or snapshot), `/__control__/snapshot?node=<inner id>` (and the
`captureSnapshot` path generally) returned
`{"error":"capture node is not in the live render tree"}` even though
`/tree` listed the node; the window root still captured fine. Reproduced
with `packages/core/examples/view-shader-history.tsx` on the
pre-boundary-refactor binary (0.0.55-45-g01a0ca34), so it predated the
boundary.rs extraction. The daily verification workflow rarely hit it
because animated apps invalidate their boundaries every frame.

## Cause

Captures are serviced by the paint walk reaching their node
(`composite::build_recursive` takes the node's requests at entry). All
three cached-boundary composite legs (Recording cache hit in
build_recursive; shaded and plain snapshot cache hits in
`boundary::snapshot_node`) early-returned without descending, so nodes
inside a valid cache were never visited and `fail_unserviced_captures`
failed them with the not-in-tree message. Culling already had the
counter-measure (`has_pending_captures` disables cull skips); cache reuse
had none.

## Fix

`composite::capture_pending_within` answers whether a pending capture
targets a node strictly inside a boundary's subtree (ancestor ascent per
pending id; ids are one or two dev-tool requests).
`service_captures_under_cache` then records the subtree into a DISCARDED
builder purely so the walk reaches and services the capture, and all
three cached legs call it before compositing their cache as before. The
cache is untouched by design: a dev-tool snapshot never re-rasterizes a
snapshot boundary or rotates a shader history (`previous: true` state
survives a mid-dissolve capture). The unserviced-capture failure now says
"never reached by the paint walk: not in the live render tree, or
hidden" instead of misreporting reachable nodes as absent.

Non-goal, on purpose: captures of hidden (display: none) subtrees still
fail - the walk never enters them, and pixels for an unpainted subtree
have no honest answer.

Tests: `tests/composite.rs capture_inside_cached_boundary_is_reached`
(reached via the discarded descent AND the cache still reused) and
`capture_of_unreachable_node_fails_as_never_reached`; live-verified with
view-shader-history (inner-node snapshots under the valid shaded cache).
