---
title: Surface maps on lit - normal, emissive, specular and light maps, plus a UV transform
description: lit took ONE map, the base color; every other pre-PBR slot (normal, emissive, specular mask, baked light, a UV transform) forced a hand-written shaderMaterial. Shipped as class-key options on lit with derivative-based normal mapping (no tangent layout), the glTF loader and .srtm v2 carrying normal and emissive, and the names settled against Three, Unity and Godot.
created: 2026-08-30
completed: 2026-08-31
---

# Surface maps on lit

Shipped 2026-08-31 (uncommitted). Usage lives in `packages/3d/AGENTS.md`
(materials section) and `examples/materials.tsx`; what follows is the
decisions and why.

## What shipped

`lit` gained `normalMap` + `normalScale`, `emissive` + `emissiveMap`,
`specularMap`, `lightMap` + `lightMapIntensity` and `mapTransform`
(`unlit` gained `mapTransform` alone), each one more lazily-created
class-key dimension. `litFragment`/`litShadowFragment` and the unlit
builders take the same options as booleans, and `NORMAL_MAP` joined the
composable `/glsl` constants. The glTF parser reads `normalTexture`
(+ scale) and `emissiveFactor`/`emissiveTexture` with
KHR_materials_emissive_strength folded into the factor; `createModel`
wires them into the default material and its `material(m, maps)`
callback now hands over every uploaded texture by lit() option name.
`.srtm` went to VERSION 2 (version-1 bakes are rejected; re-bake).

## Decisions, checked against Three / Unity / Godot

- **No tangent layout.** Normal mapping builds its tangent frame per
  fragment from screen-space derivatives (Three's untangented path,
  Schuler's cotangent frame; dFdx/dFdy are ES 3.00 core). Works on any
  UV-mapped geometry - generators included - with zero geometry work, no
  MikkTSpace port, no roadmap-10 dependency. The trade is mild seams on
  mirrored UVs; an `aTangent` named layout returns as a quality option
  only if a real model shows them (deliberate non-goal today, as are
  triplanar normal maps, aoMap - baked AO already rides `vertexColors` -
  and parallax/height/detail maps).
- **`normalScale` is one float** (Unity `_BumpScale`, Godot
  `normal_scale`), not Three's Vector2, whose second component exists to
  flip DirectX-style green channels; glTF mandates OpenGL-style +Y.
- **Emissive intensity folds into the color** (the `uLightColor`
  convention), and `emissive` defaults to WHITE when `emissiveMap` is
  given - fixing Three's gotcha where an emissiveMap alone shows nothing
  against the black default. `createModel` skips the emissive map when
  the factor is zero (glTF's product rule: emission off).
- **`specular` defaults to 1 with a `specularMap`** - the map is the
  strength.
- **`lightMap` is a material slot** (Three's form) sampled by `aUV2` -
  no new layout preset needed, `withAttribute` + reflection-based add()
  validation already cover open channels. Unity/Godot bake at scene
  level, but here the material picks the program. The term is ADDED to
  the light sum like the hemisphere, so a fully baked scene runs with no
  lights.
- **`mapTransform` is per MATERIAL** (Godot `uv1_offset`/`uv1_scale`,
  Unity Tiling/Offset), deliberately not Three's per-texture transform:
  a TextureId is a shared value whose sampling is creation-time state.
  One transform for all the material's uv maps; aUV2 exempt; the shadow
  twin transforms its cutout the same way (litBase is shared, so that
  was free).
- **Class-key width is not program count.** The measurement question
  this file used to carry dissolved on inspection: classes are created
  lazily per combination USED, so the program count equals the app's
  distinct material configurations, bounded by its material count, never
  by the tuple's width. The boolean tuple stays.
- Conflicts throw at lit() (dev validation policy): normalMap x
  triplanar, mapTransform x triplanar, mapTransform with nothing to
  transform.

2d consistency: the `color`-multiplies-map semantics match 2d's `tint`
contract (unified 2026-08-30); the names stay domain-local (`color` is
Three's, `tint` is sprite vocabulary). `setMeshParams`-driven scrolling
is the 3d analog of 2d's setSprite-from-onFrame escape hatch.

## Verified

`srt check` (15 entries), gltf-check (parse + encode/decode round-trip
of the new fields), geometry-check, and live snapshots of
`examples/materials.tsx` (all five maps visible, 61 fps, missedPresents
2/1747) and `examples/shadows.tsx` (cutout casters unchanged after the
litBase refactor).
