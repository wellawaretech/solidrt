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

## Landed (2026-08-28, uncommitted)

`shadow: { cascades: N }` (1..`MAX_CASCADES` = 4, default 1 = the box)
on a directional light. The scene keeps N internal views for it, each a
tile of the shadow atlas (so the pass count is unchanged, the fill is N
times), fitted in `placeShadowCamera` whenever the scene camera or the
light moves: the camera frustum is sliced near..far with the practical
split (halfway between uniform and logarithmic), each slice's bounding
sphere becomes an ortho box along the light with its centre snapped to
the map's texel grid (no swimming), and the box reaches back toward the
light by the whole range so casters outside the slice still cast.

The receiving side: `SHADOW_SLOTS` is indexed by MAP slot
(`uShadowRect[M]`, `uShadowMatrix[M]`, `M = MAX_SHADOW_MAPS = MAX_LIGHTS
* MAX_CASCADES`) with per-light `uShadowFirst[i]`/`uShadowCount[i]`
(count 0 replaced `uShadowCast`), and `lightShadow` walks a light's maps
tightest-first and samples the first that has the point (`SHADOW` is
now three steps, `shadowPoint` / `shadowInside` / `shadowSample`, with
`shadow()` their composition, so a map is projected once per fragment). Selection by projection rather than by view depth: no
split distances or camera-depth varying in the receiving shader, custom
materials compose the same three blocks unchanged, up to N mat4
products per fragment for the light. Views sample the same maps, fitted
to the scene camera, not their own.

`examples/cascades.tsx` is the scene that outgrows the box: a 260-unit
field of pillars under a flying camera, a click cycling 1..4 cascades
(1 = the box widened over the field, blocky everywhere). Verified on
Linux through `/gpu` (one `cascades-shadow-atlas`, three 1024 tiles,
`uShadowCount [3, 0, 0, 0]`, one atlas pass per frame) and by capture:
the near pillar's shadow edge is crisp at 3 cascades and blocky at 1.

Cost on the SM-T500 (Adreno 610), the example at 720x720 4x MSAA with
300 casters and the camera flying, `/stats` deltas over 8 s per setting:

| cascades | passes/frame | ms/frame |
|---|---|---|
| 1 (box) | 1.00 | 26.3 |
| 2 | 2.01 | 33.2 |
| 3 | 2.00 | 38.1 |
| 4 | 2.01 | 40.6 |

The box over a static caster set never re-renders its map (the shadow
camera is fixed, nothing moves), so the frame is the scene pass alone;
a cascaded light re-fits and re-renders its tiles every frame the camera
moves, which is every frame here, and each cascade is another 1024x1024
of depth fill plus one more mat4 product per receiving fragment. That
is the price of shadows that follow the camera, not of the atlas
mechanics (still one pass). No ballast was used: the frame sits well
above vsync at every setting.

Also landed the same day: the blend band (`SHADOW_BLEND` = 0.1 in map
units in `SHADOW_LOOKUP`: inside the outer 10% of a cascade's map the
factor fades into the next cascade's where that one reaches - never on
the near rim at the camera's feet, which the next cascade does not
cover - one extra PCF for those fragments only; the last map and a box
light have no band) and `shadow.distance`
(the cascades span near..min(far, distance), a point past it is lit,
default null = the camera far; the `cascades` debug command in the
example takes `{ count, distance }`). Verified by A/B capture at
`distance: 60` with the constant at 0.0 and 0.2: the two images differ
only in the strip where the big shadow crosses the cascade 0/1 hand-over
(433 pixels), nothing elsewhere. The seam this scene had was subtle
(neighbouring maps differ by ~3x, edges nearly parallel to the border),
so a harsher scene is still the human check.

The fit's pure pieces live in `math.ts` (`cascadeSplit`,
`frustumSliceSphere`, `snapToGrid`) with `packages/3d/tests/cascade.test.ts`
(`bun test packages/3d/tests`, the repo's first JS test) pinning them:
every slice corner inside its sphere, the far ring touching it, the snap
on-grid within a step; `placeShadowCamera` is policy only, and one
`forEachShadowSlot` deals the map slots for rects and matrices alike.

Open here: split ratios (`CASCADE_SPLIT_LAMBDA` is fixed at 0.5;
`shadow.splits` as fractions of the distance would be the additive
step), on demand.

## Done looks like

`shadow: { cascades: 3 }` on a DirectionalLight makes its shadow follow
the scene camera: sharp at the feet, present at the horizon, with a
seam-free transition (a blend band between cascades). The box stays the
default and the honest tier for a bounded scene; cascades are for a
scene that outgrows it, which none of the examples or demos does yet.
