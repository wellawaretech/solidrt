---
title: Cascade split ratios are fixed
description: A cascaded sun slices its range with one fixed practical split (CASCADE_SPLIT_LAMBDA 0.5), so a scene whose detail sits far from the camera (a high viewpoint, a driving game) cannot push resolution outward and a close-quarters one cannot pull it in; Godot's shadow_split_1..3 and Unity's cascade splits are per-light ratios, Three's CSM addon a mode switch.
created: 2026-09-06
---

# Cascade split ratios are fixed

Symptom: `shadow: { cascades: N }` ([3d-shadow-cascades](../done/3d-shadow-cascades.md))
places the N slice boundaries by the practical split, halfway between
uniform and logarithmic, with `CASCADE_SPLIT_LAMBDA` a constant in
`scene-shadows.ts`. Every scene gets the same distribution: a game seen
from a high camera wastes the sharp near cascade on empty ground, a
corridor game wastes the far ones on nothing. There is no per-light
knob.

Every engine with cascades exposes the split: Godot `shadow_split_1`,
`_2`, `_3` (fractions of the shadow distance, per light), Unity the
cascade splits in the quality settings (fractions, plus a "shadow
distance"), Three's CSM addon a `mode` of uniform / logarithmic /
practical / custom with `customSplitsCallback`.

## Shape

`shadow.splits?: number[]` on a directional light: `cascades - 1`
fractions of `shadow.distance` in ascending order, the slice boundaries
(Godot's form; Unity's is the same numbers). Given, they replace the
lambda; absent, the practical split stays the default. Validation
throws on the wrong length or a non-ascending list. `cascadeSplit` in
`math.ts` already takes the lambda as an argument, so the change is in
`placeShadowCamera` and the option type, plus a test row in
`packages/3d/tests/cascade.test.ts`.

## Done looks like

`examples/cascades.tsx` gains a splits setting beside its cascade count,
and pulling the first boundary in sharpens the pillars at the camera's
feet at the visible cost of the horizon.
