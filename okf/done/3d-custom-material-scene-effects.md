---
title: Scene-wide effects on custom materials - one answer for fog, shadows and what comes next
description: A shaderMaterial got the scene's fog, shadows, lights and output only by composing each set itself, so an instanced forest stayed crisp and unshadowed in a fogged, shadowed scene and a hand-rolled loop rendered a spot as a directional light, with no error. Decided as a function-level contract, neither injection nor bare composition - one scene set (sceneSource) with a Surface struct, shade functions, a light accessor and one output tail, the stock materials built from it, custom looks joining at one of three tiers.
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

## The decision

Neither shape as framed. The field (Godot's `fragment()` outputs,
Filament's `material()`, Unity URP's `UniversalFragmentPBR` + `MixFog`,
Bevy's `apply_pbr_lighting` + post-lighting processing, drei's
CustomShaderMaterial for Three) converges on a FUNCTION-LEVEL contract:
the author's code is a function the package's program calls (that is
composition, no string surgery), and the package owns the effects (what
injection wanted). Landed 2026-09-06 as `sceneSource` in
`@solidrt/3d/glsl`: one set declaring everything the scene binds, a
`Surface` struct (URP's SurfaceData, one for both light models),
`shadeBlinn` / `shadePbr` (the whole light loop, environment and
emissive), the `sceneLight(i, position, normal)` accessor (URP's
GetAdditionalLight - the light's direction and its color already
attenuated, cone-faded and shadowed, so the spot-as-directional mistake
cannot be written) and `sceneOutput` (fog, exposure, tone mapping,
encode - the one tail, which is also where the HDR scene buffer will
land). The lit, standard, unlit and sprite fragments are built from that
set, checked byte-identical before and after by
`probes/scene-set-probe.tsx` (every material variant under a sun, a
spot, a point light, fog, an environment and tone mapping). Custom looks
join at three tiers - a stock fragment on a custom vertex stage, a
`surface` function slot (which replaced `discardIf`), or a fragment of
their own over the set - documented in `packages/3d/AGENTS.md` with the
premise that was right all along: what you declare is what runs.

The exposure/tone-mapping/encode tail turned out to be a fourth thing
custom fragments forgot (the demo's knot wrote a hand gamma to
fragColor), which is what settled the tail as one owned function.

## The two shapes, as first framed

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

Cut into `packages/3d/AGENTS.md` (the tiers paragraph under the GLSL
exports, and the trap "a custom fragment that ends in `fragColor =
vec4(...)` bypasses the scene"). The spot-as-directional finding of
2026-09-02 is the trap's second sentence; its repro
`probes/spot-custom-material-probe.tsx` is now the tier-3 rig, a
`sceneLight` loop beside a lit() floor that must match it.

## Done looks like

`examples/instanced.tsx` fogs and shadows with the standard meshes
beside it - it lost its fragment altogether (tier 1). No engine change.
