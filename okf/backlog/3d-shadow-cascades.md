---
title: Cascaded shadow maps
description: One shadow.camera box per casting light: a large outdoor scene either blurs (the box covers everything at one map's resolution) or clips (the box covers the near part and the far part is lit). Cascades split the view frustum into N maps; demand-gated on a scene that outgrows the box.
created: 2026-08-27
---

# Cascaded shadow maps

Symptom: a directional light's shadow frustum is a fixed orthographic
box (`shadow.camera`, +-5 units by default) in
[3d-shadow-maps](../done/3d-shadow-maps.md). Widening it to cover a
landscape spreads one `mapSize` over the whole area, so contact shadows
near the camera turn to blocks; keeping it tight leaves the distance
unshadowed. Three's `DirectionalLightShadow` has the same box and the
same limit; Godot and Unity answer with cascades.

Cascaded shadow maps render the same light from two to four boxes fitted
to slices of the VIEW frustum (near slice small and sharp, far slice
large and coarse), and the receiver picks the cascade by view depth.
Everything needed exists: a shadow is a view, a light can own several
views, the slot set is per light - the cascade set would be N maps and
N matrices under one light slot, plus a split-depth array and a
per-fragment cascade select in `SHADOW_LOOKUP`.

## Done looks like

`shadow: { cascades: 3 }` on a DirectionalLight makes its shadow follow
the scene camera: sharp at the feet, present at the horizon, with a
seam-free transition (a blend band between cascades). The box stays the
default and the honest tier for a bounded scene; cascades are for a
scene that outgrows it, which none of the examples or demos does yet.
