---
title: Light layers
description: Meshes and targets have layer masks but lights are scene-wide, so an outside light lights an enclosed interior straight through its shell and an interior light leaks out; a layers mask on every light, matched against mesh layers in lightVector and applied to the light's shadow views, as Unity's cullingMask and Godot's light_cull_mask.
created: 2026-09-22
completed: 2026-09-26
---

# Light layers

Symptom: a model with an interior the camera enters. The scene's layer
mask already hides the outside from inside and the inside from outside
(the sealed-interiors note in AGENTS.md "Views and layers"), but every
light reaches every mesh: the sun lit the interior through the shell,
and the interior lights would light the outside. The app blended each
light's intensity by how deep the eye was inside, which is a fade, not
a mask, and it depended on the other side's meshes being masked out.

Unity (`Light.cullingMask`) and Godot (`light_cull_mask`) both have
this; Three does not.

Context: `packages/3d/src/light.ts` (the light types and options, no
mask today), `scene.ts` `writeLights` (packs the light list into the
shared `uLight*` arrays per target, see `LIGHT_SLOTS` in `glsl.ts`),
the light loop in `glsl.ts` (`lightVector`, the loop to `uLightCount`
in the lit fragments and `sceneLight`), the shadow views per light
(`scene-shadows.ts` `createShadow`, per-light already, filter
`m => m.castShadow`, mask `sceneMask`), and mesh `layers` (a
membership bitmask the targets test against in JS; it never reaches
the shader).

Why the shader mask and not per-object light lists (Unity built-in
forward): the light list is written once per target however many
meshes exist, and `uLightDir`/`uLightPos` are core-driven slots JS
cannot remap per mesh; per-mesh lists would fan every light change out
to every entry for a negligible loop saving at MAX_LIGHTS = 8. The
mask test is uniform control flow (both operands are uniforms), so the
branch is coherent per draw and effectively free.

Done looks like:

- `layers?: number` on every light option and type, default all bits
  set (a light lights everything unless told otherwise, the Unity and
  Godot default), validated by `checkMask`, live through
  `setLight({ layers })` and the component props
  (`<PointLight layers>` etc.). `layers` as a FILTER mask follows the
  precedent views set, against mesh `layers` membership.
- `LIGHT_SLOTS` gains `uniform int uLightMask[MAX_LIGHTS]` and the
  per-mesh `uniform int uLayers`; the hemisphere takes `uHemiMask`
  beside `uHemiSky`/`uHemiGround`. Declaring them in the set means no
  custom lit source forgets them.
- The gate lives at the top of `lightVector`, not in each loop:
  `(uLightMask[i] & uLayers) == 0` returns 0.0, which is already the
  documented "cannot reach the fragment" contract - `sceneLight`'s
  `a > 0` gate then skips the shadow tap, the stock loops skip the
  terms, and raw `LIGHT_LOOKUP` users are covered by the same line.
  The shade functions gate the hemisphere term by
  `(uHemiMask & uLayers) != 0` (an interior gets its own ambient back
  through `Surface.ambient`).
- The int32 boundary: masks are validated 0..0xffffffff but GLSL ES
  3.0 highp int is signed, so every mask crosses to the uniform as
  `mask | 0` (all bits = -1; the AND is on bit patterns and the
  `== 0` test is sign-agnostic). Nothing switches to uint.
- `writeLights` pushes the masks with the rest (one write per light
  change, every receiving target); `uLayers` is a per-entry param
  seeded from `mesh.layers` in the entry seed for the scene entry and
  every non-override view entry (an unwritten int uniform reads 0,
  which would kill ALL lights, so the seed is unconditional), and a
  `setLayers` rewrite rides the existing `_setParams` fan-out. Shadow
  and other override views are unlit and carry no `uLayers`.
- The light's shadow views get mask `sceneMask & light.layers`
  instead of `sceneMask`: `attachView` already tests
  `mesh.layers & v.mask`, so a mesh the light cannot see casts
  nothing into its map and mesh `setLayers` re-filters on its own. A
  `setLight({ layers })` on a casting light rewrites its existing
  shadow views' masks and re-runs attach/detach (the `view.setLayers`
  path is the pattern) - NOT through `_shadowChanged`/settle, which
  would rebuild atlas tiles for what is only a re-filter.
- Picking, culling and the draw sort are untouched; a light's own
  node keeps no mask semantics beyond this. An instanced population
  shares its group's `layers`, so masking is per mesh, not per
  instance - consistent with views and with Unity's per-renderer
  mask. A later `shadowLayers` split (Unity has one) stays additive.
- AGENTS.md "Lights": the option, the default, and the
  sealed-interiors note updated to say lights and the hemisphere mask
  the same way - and PLAINLY that the environment term does not:
  see the companion item below. A probe with a lit box around a lit
  room: `/texture` of the scene shows the inner faces unlit by the
  outside light and the outer faces unlit by the inside one.

Out of scope, filed as its own item: the environment cube leaks the
same way (`envIrradiance`/`envRadiance` from the scene-wide `uEnv`
light an interior with the sky no matter the light masks) - a
per-mesh environment assignment is a different mechanism
(`3d-env-per-mesh.md`, done 2026-09-26).

The mesh-side `uLayers` write is the cost to watch: one param per
entry, written on attach and on `setLayers`, never per frame.

Implemented 2026-09-26 exactly as above (the mesh-side flag landed as
`Material.layered`, detected from the source like `lodFade`; the
shadow view records carry their `shadowLight` and the scene derives
the caster filter, so `setLight({ layers })` re-masks without a tile
rebuild). Verified live with a two-box/two-sun/masked-hemisphere probe
(`light-layers-probe.tsx` at the repo root): per-light color and
shadow separation, the hemisphere gate, and all three live write paths
(sun re-mask growing a second shadow, mesh `setLayers` flipping its
shadow to the other sun's map, hemisphere re-mask); `materials.tsx`
(shadePbr) and the tier-3 demo render unchanged under default masks.
