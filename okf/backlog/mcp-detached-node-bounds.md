---
title: get_render_tree reports useless boxes for detached nodes
description: A d-* node has no layout entry, so the tree reports the box it inherits from its nearest layout ancestor - a d-line spanning (10,120)-(200,120) came back as 1692x1128 - which is correct per the model and useless for locating anything in a d-*-heavy app.
created: 2026-08-02
---

# get_render_tree reports useless boxes for detached nodes

What it looks like when you hit it: an agent reads the render tree of a
drawing-heavy app and every d-* node reports the same enormous box. A `d-line`
spanning (10,120)-(200,120) was reported as width 1692, height 1128 - the box
it inherits from the nearest layout ancestor. That is correct per the
documented model and useless for finding the node, and an app that draws with
d-* is d-*-heavy by nature.

The engine already computes what is wanted: `captureSnapshot` sizes d-*
captures from `local_bounds` (see
[capture-detached-nodes](../done/capture-detached-nodes.md)). Surface the same
quantity as a `drawn` box alongside the inherited one, so the tree can locate
the node without changing what the existing box means.

From the animated-explainer demo feedback. Split out of a five-part round-2
agent dev-loop feedback item when okf was restructured; the siblings are
[mcp-multi-client-ergonomics](mcp-multi-client-ergonomics.md) and
[mcp-interaction-perf-visibility](mcp-interaction-perf-visibility.md).

Resolved 2026-08-27, without a separate `drawn` box: `Line` (with
[line-points](../done/line-points.md) stage 3) and `Path` implement `Bounded`, so
`bounding_box_viewport` - what the tree box, `getBoundingBox` and a detached
capture read - reports the geometry plus the stroke's reach for `d-line` and
`d-path`. A path's extent is the tight one (curve extrema, via lyon's
`bounding_box`) at the node's x/y, and both kinds are now culled by that box
where path used to be unbounded. The other d-* kinds already reported their
own w/h.
