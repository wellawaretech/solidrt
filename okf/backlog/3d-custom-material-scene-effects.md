---
title: Scene-wide effects on custom materials - one answer for fog, shadows and what comes next
description: A shaderMaterial gets the scene's fog and shadows only by composing FOG and the SHADOW trio itself, and every instanced mesh has a custom material, so an instanced forest stays crisp and unshadowed in a fogged, shadowed scene with no error. Decide once between injecting the standard tail at the fragColor write and exporting one composed function the author calls, before the next scene-wide effect adds a third thing to forget.
created: 2026-08-30
---

# Scene-wide effects on custom materials - one answer for fog, shadows and what comes next

## Symptom

The standard materials (`unlit`, `lit`, `sprite`) compose the scene's
fog (`FOG`, 2026-08-30) and `lit` composes the shadow set
(`SHADOW_SLOTS`/`SHADOW`/`SHADOW_LOOKUP`). A `shaderMaterial` composes
nothing unless its author does: it declares what it reads, and what it
declares is what runs - the package's standing rule, and the right one.

The consequence is a silent gap that grows with every scene-wide effect.
An instanced mesh always has a custom material (`instanceAttributes`
forces `shaderMaterialClass`), so the AGENTS.md billboard recipe,
`examples/instanced.tsx`, a chunk-streamed forest, a particle fleet - the
populations a game has most of - stay crisp in a fogged scene and
unshadowed in a lit one, with no warning, because nothing is wrong at the
engine level. The trap is documented in `packages/3d/AGENTS.md` (Traps,
"scene-wide effects reach a custom material ONLY by composition"), which
is where it should live until this item decides the mechanism. Three has
the same split (a `ShaderMaterial` needs the fog and shadow chunks), and
Three users hit it constantly.

## The two shapes

- **Injection.** `shaderMaterialClass({ fog: true, receiveShadow: true })`
  rewrites the fragment source: declares the sets and wraps the
  `fragColor` write in the standard tail. Zero-effort for the author,
  and the class options then mean the same thing on every material.
  Costs: textual surgery on app-authored GLSL (find the write, handle
  early returns and multiple writes, keep the line numbers right for
  [glsl-line-injection](glsl-line-injection.md)), and a material that
  looks declarative but has code the author never saw - the first
  "why does my shader do that" that cannot be answered by reading it.
- **Composition, made one line.** Export one composed function per
  scene-wide tier - `sceneShade(rgb, alpha, worldPos, normal)` or two
  (`sceneLight`, `sceneFog`) - so a custom fragment ends in one call
  and gets fog, shadows, and whatever the scene grows next, without
  knowing the pieces. The author still writes the line; the package
  owns what it does. `lit` itself would be built from it, so the
  standard and custom paths cannot drift.

The second matches the exported-GLSL policy (roadmap item 2: "custom
materials never become second-class" by sharing SOURCE, not by magic)
and keeps "what you declare is what runs". The first is what Unity's
surface shaders were, and Unity retired them. Lean composition; decide
when the next scene-wide effect arrives (point lights, environment
maps) so the function's signature is designed against three consumers,
not two.

## Findings

- The third thing to forget arrived with spot and point lights
  (2026-09-02), and it is worse than fog or shadows: a custom fragment
  that hand-rolls the directional pattern (`lambert(n, uLightDir[i])`
  times `lightShadow(i, ...)`, no `lightVector`) renders a SpotLight as
  a directional light - no cone, no falloff - so the "spotlight" washes
  every mesh it faces edge to edge and a floor plane reads as a lit
  RECTANGLE instead of a pool. Both of the demo's shaders
  (`the-third-dimension.tsx`) are this exact pattern. Repro:
  `probes/spot-custom-material-probe.tsx`, cone-less custom floor beside
  a `lit` floor, same spot. Verified correct in `lit` on desktop GL and
  Adreno (`probes/spot-point-probe.tsx`). The composed-function design
  now has its three consumers: fog, the shadow trio, and the light
  loop itself (`lightVector` gating both attenuation and whether the
  shadow lookup runs at all, the `a <= 0.0 continue` in `lit`).

## Done looks like

`examples/instanced.tsx` fogs and shadows with the standard meshes
beside it, its fragment one line longer; AGENTS.md's trap paragraph
shrinks to "end a custom fragment with `sceneShade`". No engine change.
