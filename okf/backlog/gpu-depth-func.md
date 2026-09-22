---
title: Depth func option
description: The depth comparison is fixed at LESS with no override, which blocks equal-depth multi-pass tricks (LEQUAL) and reversed-z; Three's Material.depthFunc (default LessEqualDepth) and Unity's ZTest are the same knob, so a depthCompare option on createRenderPipeline is a parity gap, additive with the default staying less. Split from gpu-pipeline-extensions 2026-08-11.
created: 2026-08-11
---

# Depth func

Symptom: the depth comparison is fixed at `LESS`. A pipeline cannot express
`LEQUAL` (redrawing geometry at equal depth in a later pass), `GREATER`
(reversed-z precision setups), or `ALWAYS`/`EQUAL` tricks. Three exposes
exactly this as `Material.depthFunc` (and defaults to `LessEqualDepth`,
so a Three port drawing coplanar decals or a second pass over the same
geometry silently relies on it), Unity as the shader's `ZTest`; Godot
stops at `depth_test_default` / `depth_test_inverted` (LESS or GREATER,
nothing else), the one engine without the full set.

The shape when one arrives: `depthCompare` on `createRenderPipeline` next
to `depth`/`depthWrite`, WebGPU's vocabulary
(`"less" | "less-equal" | "greater" | ...`), purely additive - the default
stays `"less"`.

Likely first consumer: shadow maps (roadmap item 15 in
[3d-roadmap](../notes/3d-roadmap.md)) name it together with
[gpu-sampleable-depth](../done/gpu-sampleable-depth.md); depth-func alone is not
the blocker there. The shaped plan ([3d-shadow-maps](../done/3d-shadow-maps.md))
does not depend on it at all: `LESS` serves both the depth pass and the
main pass, so this stands on its own, as the parity item it is.

History: deferred raster-state remainder of
[gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md) (cull mode landed
2026-08-04); split out 2026-08-11.
