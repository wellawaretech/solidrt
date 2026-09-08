---
title: The standard vertex prefix has no opt-out, so a point cloud pays double
description: Every layout must start with aPos/aNormal/aUV, 8 floats a vertex, but a lidar point is 4 (position plus one packed channel) and has neither a normal nor a UV; a 14.3M-point cloud therefore carries 458 MB of vertex buffer for 229 MB of data, structurally unusable.
created: 2026-09-08
---

# The standard vertex prefix has no opt-out, so a point cloud pays double

## Symptom

A geometry's layout "must begin with the standard prefix (aPos vec3,
aNormal vec3, aUV vec2)", enforced in `packages/3d/src/geometry.ts`
(`checkLayout`). That is 8 floats a vertex minimum. A lidar point is 4:
a position and one packed attribute. It has no normal (a fitted one is
per-neighbourhood, not per-vertex, and its sign is arbitrary) and no UV
(a point sprite is shaded from `gl_PointCoord`).

A 14.3M-point cloud therefore uploads 458 MB to carry 229 MB, and the
other 229 MB can never be read by anything.

## Cause

The prefix is what lets one shader vocabulary serve every geometry: a
material's vertex stage names `aPos`/`aNormal`/`aUV` and any geometry
satisfies it. That reasoning is sound for surfaces and is the reason
the rule exists. It simply has no give for geometry that is not a
surface, which is exactly what `topology: "points"` (and `"lines"`) is
for. The topology work of 2026-09-08 made non-triangle geometry a
citizen everywhere else - it picks by bounds, casts no shadow, skips
`createShape` - and this is the piece that did not follow.

## Done looks like

A geometry whose topology is not triangles may declare a layout without
the prefix, and everything downstream keeps working:

- `checkLayout` requires the prefix only for triangle topologies (a
  layout still needs `aPos` first: placement is universal).
- The stock materials keep requiring what they read, so pairing a
  prefixless geometry with `lit` still throws the ordinary
  missing-attribute error at add(). Only shader materials that declare
  what they read can use one, which is already the rule.
- Documented where the prefix is documented, with the memory figure -
  the payoff is halving a large cloud, and that is the reason to reach
  for it.

Deliberately not in scope: packed vertex formats (u8/i16 attributes).
That is a bigger vocabulary change and would cut the same cloud further,
but the prefix is the part that is pure waste.

## What it involves

`geometry.ts` (the check and the layout docs), a look at every
`layoutAttributes`/`layoutStride` consumer for a prefix assumption, and
the glTF loader, which builds layouts by name and would want to keep
emitting the prefix for surfaces regardless.
