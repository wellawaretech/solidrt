---
title: Point light shadows
description: A PointLight lit but could not cast. Landed 2026-09-02 as six 90-degree face tiles in the existing shadow atlas with a dominant-axis face select in SHADOW_LOOKUP - the Three/Godot/URP atlas route, library-only, no cube maps; a fov guard band (URP's fovBias) closes the face-seam slits.
created: 2026-08-27
---

# Point light shadows

Symptom: a bulb inside a room lit every wall but cast no shadow -
`castShadow` existed on DirectionalLight and SpotLight only. The light
model half of the old spot-and-point item (the nodes, the typed light
list, the falloff, spot shadows) had landed 2026-09-02 already; this
item was the point light's map in every direction.

## What landed (2026-09-02)

`<PointLight castShadow shadow={{ mapSize?, bias?, normalBias?, near?
}}>` (`createPointLight`, `setLight`) - the spot option set. Six
90-degree perspective face views (world-axis aligned, slot order +X,
-X, +Y, -Y, +Z, -Z) dealt as six consecutive tiles of the existing
shadow atlas, the same several-consecutive-slots-per-light shape
cascades use; far = the light's `distance` (or the spot default when
0). The lit shader picks the face from the dominant axis of the
light-to-fragment vector and projects through that slot's existing
`uShadowMatrix`/`uShadowRect` - one projection, one hardware-compare
tap (`SHADOW_LOOKUP`, which therefore composes over LIGHT_SLOTS).
`CastingLight` is now every light type; the slot budget throw covers
the six-slot claim.

Two seam measures: each face map renders `POINT_SHADOW_FOV_GUARD`
degrees wider than its face (URP's fovBias - without it every face
seam shows a lit slit where each map's rasterized coverage ends at its
edge; the select is unchanged so the overlap is never sampled twice),
and PCF taps clamp at tile edges as before, so a seam hardens slightly
instead of bleeding into a neighbouring tile.

Verified with `probes/point-shadow-probe.tsx` (a bulb between walls,
four pillars, every face receiving): continuous shadows across all
face seams, atlas dealt as a 3x2 grid of six tiles, six shadow views
sharing the caster list; moving-caster re-placement checked by
stepping a frozen clock, and the same probe rendered correctly on the
Adreno 610 tablet (cost recorded in
[3d-low-end-gpu-performance](../backlog/3d-low-end-gpu-performance.md):
~12.5 ms there, dominated by the flat per-pass overhead).
`examples/lamps.tsx` ships the feature: its orbiting bulb casts, and
with the two spots the example sits exactly at the 8-slot budget.

## Why not `samplerCube` (the shape this replaced)

The earlier decided shape rendered distance into an rgba8 cube via
[gpu-cube-maps](../done/gpu-cube-maps.md). The Three/Godot/Unity
comparison overturned it: Three (`cubeToUV` tiles in the 2D map),
Godot (omni shadows in the one atlas) and Unity URP (six tiles in the
additional-lights atlas) all ship the atlas route; only Unity's
desktop-era built-in RP uses a dedicated cube map. The atlas is also
the efficient route here: packed-distance forfeits hardware depth
compare (fetch + unpack + manual compare per tap), a depth-cube +
`samplerCubeShadow` variant needs cube depth targets and hits the GLSL
ES 3.00 constant-index rule for sampler arrays (N point casters = N
unrollable cube uniforms), while the atlas keeps every shadow type in
the one loop, uncapped, with adaptively sized tiles. Cube maps stay
demand-gated on the environment tier; a `samplerCubeShadow` fast path
for a hero light would be additive if profiling ever asks.
