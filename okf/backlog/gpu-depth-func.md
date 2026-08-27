---
title: Depth func option
description: The depth comparison is fixed at LESS with no override, which blocks equal-depth multi-pass tricks (LEQUAL) and reversed-z; a depthCompare option on createRenderPipeline is additive when a demand signal arrives. Wanted together with sampleable depth for shadow maps. Split from gpu-pipeline-extensions 2026-08-11.
created: 2026-08-11
---

# Depth func

Symptom: the depth comparison is fixed at `LESS`. A pipeline cannot express
`LEQUAL` (redrawing geometry at equal depth in a later pass), `GREATER`
(reversed-z precision setups), or `ALWAYS`/`EQUAL` tricks. No field demand
signal yet, which is why this stays deferred rather than open.

The shape when one arrives: `depthCompare` on `createRenderPipeline` next
to `depth`/`depthWrite`, WebGPU's vocabulary
(`"less" | "less-equal" | "greater" | ...`), purely additive - the default
stays `"less"`.

Likely first consumer: shadow maps (roadmap item 15 in
[3d-roadmap](../notes/3d-roadmap.md)) name it together with
[gpu-sampleable-depth](gpu-sampleable-depth.md); depth-func alone is not
the blocker there. The shaped plan ([3d-shadow-maps](../done/3d-shadow-maps.md))
does not depend on it at all: `LESS` serves both the depth pass and the
main pass, so this stays demand-gated on its own.

History: deferred raster-state remainder of
[gpu-pipeline-extensions](../done/gpu-pipeline-extensions.md) (cull mode landed
2026-08-04); split out 2026-08-11.
