---
title: Light layers
description: Meshes and targets have layer masks but lights are scene-wide, so an outside light lights an enclosed interior straight through its shell and an interior light leaks out; a layers mask on every light, matched against mesh layers in the light loop and applied to the light's shadow view, as Unity's cullingMask and Godot's light_cull_mask.
created: 2026-09-22
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
in the lit fragments and `sceneLight`), the shadow views per light,
and mesh `layers` (a membership bitmask the targets test against in
JS; it never reaches the shader).

Done looks like:

- `layers?: number` on every light option and type, default all bits
  set (a light lights everything unless told otherwise, the Unity and
  Godot default), live through `setLight({ layers })` and the
  component props (`<PointLight layers>` etc.).
- The light list gains `uniform int uLightMask[MAX_LIGHTS]`, written
  with the rest by `writeLights`; the hemisphere light takes a mask the
  same way (`uHemiMask`).
- Each mesh entry carries `uniform int uLayers`, seeded from
  `mesh.layers` and rewritten by `setLayers`, on every lit material and
  on `SCENE`/`sceneLight` for tier-3 materials (declared in the scene
  set so no custom source forgets it). Unlit materials do not declare
  it.
- The loop skips a light whose `(uLightMask[i] & uLayers) == 0` before
  its shadow lookup, so an excluded light neither lights nor shadows
  the mesh; GLSL ES 3.0 integer ops, one AND per light per fragment.
- The light's shadow views draw casters by `sceneMask & lightMask`: a
  mesh the light cannot see casts nothing into its map.
- Picking, culling and the draw sort are untouched; a light's own node
  keeps no mask semantics beyond this.
- AGENTS.md "Lights": the option, the default, and the sealed-interiors
  note updated to say lights mask the same way. A probe with a lit box
  around a lit room: `/texture` of the scene shows the inner faces
  unlit by the outside light and the outer faces unlit by the inside
  one.

The mesh-side `uLayers` write is the cost to watch: one param per
entry, written on attach and on `setLayers`, never per frame.
