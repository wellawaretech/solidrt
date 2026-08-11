---
type: backlog-item
title: Alpha translucency (sorted blending)
description: Blending within a draw is additive-only, so non-convex transparent meshes and per-particle-colored accumulation have no path (the convex workaround splits front/back faces into two composited targets). The mode itself is trivial; what defers it is sorted geometry and the straight-vs-premultiplied answer against Impeller's compositing. Split from gpu-pipeline-extensions 2026-08-11.
status: open
timestamp: 2026-08-11T00:00:00Z
---

# Alpha translucency

Symptom: transparent geometry has no general path. Blending within one draw
is additive-only (`blend: "add"`, landed 2026-07-29); classic alpha
translucency (`SRC_ALPHA, ONE_MINUS_SRC_ALPHA` or its premultiplied form)
is not offered. The known workaround is convex-only: front and back faces
split into two targets composited with `<texture blendMode="plus">`, which
works only because a convex object has exactly one front and one back face
per pixel. Non-convex transparent meshes and many-particle accumulation
with per-particle colour still have no path (demand recorded 2026-07-29 in
[gpu-pipeline-extensions](gpu-pipeline-extensions.md)).

Adding the blend mode is trivial; what defers it is correctness, per
[gpu-pipeline-blend-modes](gpu-pipeline-blend-modes.md) (the fuller design
note for the blend vocabulary): it needs sorted geometry - which the draw
list's ordering verbs (`before`, `setDrawOrder`) now make expressible, but
no sorting story owns - and an answer to straight-vs-premultiplied against
how Impeller composites the target. Do not add the mode without deciding
those two.

First step regardless, and it costs nothing: document the target pixel
contract (premultiplied, non-linear RGBA8 -
[gpu-pixel-contract-docs](gpu-pixel-contract-docs.md)), which answers the
straight-vs-premultiplied half by declaring it.

History: the remaining half of the blending bullet in
[gpu-pipeline-extensions](gpu-pipeline-extensions.md) (additive half done
2026-07-29); split out 2026-08-11.
