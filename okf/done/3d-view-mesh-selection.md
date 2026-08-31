---
title: Per-view mesh selection (Three's layers) and the scene's own depth texture
description: createView mirrors EVERY mesh, so a minimap cannot show markers only, a rear-view mirror cannot leave out the HUD meshes and a reflection view draws the reflector itself; the mesh filter exists internally for shadow views and is not public. Expose it on ViewOptions, and widen depth "texture" to the scene's own target so a depth-reading post effect has an input.
created: 2026-08-30
completed: 2026-08-31
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

Settled 2026-08-31 against Three (`object.layers` + `camera.layers`),
Godot (`layers` + `cull_mask`) and Unity (`layer` + `cullingMask`), which
all agree: membership on the OBJECT, mask on the camera/view. A predicate
on the view was considered and rejected - it needs a refresh() API, makes
the app tag meshes anyway for the predicate to test, and cannot exclude a
mesh from the MAIN render (the minimap's markers must not draw in the
scene itself), which needs a mask on the scene's own target exactly as
Three does through the rendering camera.

- `layers?: number` on Mesh (bitmask, default 1), `setLayers(mesh,
  bits)`, `<Mesh layers>`. Not inherited from Groups (Three's and
  Godot's rule).
- A `layers` mask on `SceneOptions` and `ViewOptions` (default 1,
  Three's camera default - everything visible out of the box), driven
  live by `setLayers` on the scene handle and each view. Membership is
  evaluated at attach; a mask or mesh change attaches/detaches the
  delta entries.
- Shadow views follow the scene's mask (what the scene cannot see must
  not darken it); `scene.pick`/`raycast` skip masked-out meshes, the
  same rule as invisible ones.
- The "layer" noun collision with 2d's SpriteLayer/TileLayer containers
  is accepted: Godot ships CanvasLayer beside `layers` masks in one API,
  and a future 2d view takes the same bitmask shape
  ([2d-layer-views](2d-layer-views.md)).

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

## Delivered

2026-08-31, all three pieces (uncommitted): mesh `layers` bitmask +
`setLayers` + the `layers` props, target masks on SceneOptions/ViewOptions
with `setLayers` on scene and view (shadow views follow the scene's mask,
pick/raycast skip masked-out meshes), `depth: "texture"` +
`scene.depthTexture` on the scene target (throws with `samples`), per-view
`fog` and the view-owned-params precedence rule. Documented in
`packages/3d/AGENTS.md`; `examples/scene-views.tsx` shows the marker
layer and the unfogged map, verified by snapshot.

## Done looks like

`examples/scene-views.tsx` grows a minimap view whose `layers` mask
admits marker meshes only (markers masked out of the main render),
`fog: null` on it while the scene is fogged; an SSAO or depth-fog post
effect in `output` samples `scene.depthTexture` with no second pass.
