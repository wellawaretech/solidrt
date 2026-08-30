---
title: Per-view mesh selection (Three's layers) and the scene's own depth texture
description: createView mirrors EVERY mesh, so a minimap cannot show markers only, a rear-view mirror cannot leave out the HUD meshes and a reflection view draws the reflector itself; the mesh filter exists internally for shadow views and is not public. Expose it on ViewOptions, and widen depth "texture" to the scene's own target so a depth-reading post effect has an input.
created: 2026-08-30
---

# Per-view mesh selection (Three's layers) and the scene's own depth texture

## Symptom

`scene.createView` mirrors one entry per mesh into the view's target
([scene.ts](../../packages/3d/src/scene.ts), `makeView`); the only
selection it has is `overrideMaterial` (one material for all) and a
`filter: (mesh) => boolean` that is INTERNAL, used by the shadow views to
admit `castShadow` meshes only. Public `ViewOptions.filter` is the
texture sampling filter, a different thing under the same name.

Three has `object.layers` and `camera.layers`; Godot has visual layers
and cull masks. The consumers in a racing game are the ordinary ones:

- A minimap view drawing the track outline and kart markers, not the
  scenery.
- A rear-view mirror leaving out the player's own kart body (or drawing
  the world without the first-person HUD meshes).
- A planar reflection view that must not draw the reflecting surface
  itself.
- A debug view of collision volumes only.

Today each of these is a second scene with duplicated meshes and its own
per-frame transform writes, exactly the doubling views were built to
avoid.

## Shape

Additive on `ViewOptions`: `include?: (mesh: Mesh) => boolean`, the
existing internal filter made public under a name that does not collide
with the sampling `filter`. Evaluated at view creation for present meshes
and at add() for later ones (the shadow-view rule). Re-evaluated on an
explicit `view.refresh()` rather than per frame - a mesh that changes
class re-adds through `setGeometry`/`setMaterial` already, so the common
case is static. A `layers` bitmask (Three's spelling) is a convenience
over the predicate, not a second mechanism; decide the spelling once
against Three/Godot/Unity and keep one.

Same file, second small gap: `depth: "texture"` lives on `ViewOptions`
only. `SceneOptions` has no depth option, so the scene's own pass keeps
its depth in a buffer and a post effect in `output` that wants it (SSAO,
depth of field, fog by depth, soft particles later) has no input except
a second full view with the depth override - a whole extra pass over the
scene for a texture the main pass already wrote. Widen `SceneOptions.
depth?: true | "texture"` and expose `scene.depthTexture`, the view
shape exactly; the roadmap's "post-processing is `output` + shader
chains" stance holds only once this exists.

Third, on the same `ViewOptions`: `fog`. The scene's fog is a
`setParams` fan-out, so every view inherits it - a top-down minimap or
an override-material debug view is fogged like the main camera. Today
`view.setParams({ uFogInv: 0, uFogDensity: 0, uFogHeightFalloff: 0 })`
clears it, but the next `scene.setFog` writes over the view (same
target, last write wins). Two additive pieces: `fog?: FogOptions | null`
on `ViewOptions` (null = an unfogged view; absent = follow the scene),
and the rule that a view's OWN params win over the scene fan-out - the
scene replays its names to a view only for names the view has not set
itself. State that ordering in the `setParams` docs either way.

## Done looks like

`examples/scene-views.tsx` grows a minimap view with `include` admitting
marker meshes only, `fog: null` on it while the scene is fogged; an SSAO
or depth-fog post effect in `output` samples `scene.depthTexture` with no
second pass.
