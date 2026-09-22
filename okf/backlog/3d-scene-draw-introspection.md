---
title: No way to ask why a mesh did not draw
description: Four independent mechanisms silently drop a mesh from a target - frustum culling, the layer mask, overrideMaterial skipping instanced meshes, and the shadow view's caster filter - and the scene exposes no introspection at all, so the only diagnosis is a screenshot and a guess.
created: 2026-09-08
---

# No way to ask why a mesh did not draw

## Symptom

A mesh is in the scene and nothing appears. Today's answer is to snapshot
the target and reason backwards, because four independent mechanisms drop
an entry without a word:

- frustum culling (the core's cull group, per target),
- the layer mask (`mesh.layers & view.mask`),
- `overrideMaterial` skipping instanced meshes whose record layout the
  override cannot declare (scene.ts, the `v.override !== null && inst
  !== null` branch),
- the shadow view's caster filter, plus the lines-and-points rule that
  makes non-triangle topology cast nothing.

`get_render_tree` answers the same question for the UI tree. The scene
graph has no equivalent: `ScenePublic` carries no introspection call, so
"is it there, and did it draw" is unanswerable from inside the app or over
MCP. Reported after a second view with an `overrideMaterial` came up
empty because every mesh in it was instanced - the behaviour is
documented and was read, but invisible at the moment it mattered.

## Done looks like

A `scene.debugEntries()`-shaped read: one row per mesh, and per row the
targets it is drawn into plus the reason it is absent from the others -
visible / culled / layer-masked / override-skipped / not a caster. Dev
surface, allocation allowed: this is a debugging call, not a frame path.

Then the same data over MCP, so an agent can ask without editing the app -
`get_render_tree`'s counterpart for scenes, or a scene section on it.

Involves: `packages/3d/src/scene.ts` (the per-view entry walk already
computes every one of these reasons; the item is recording them instead of
returning early), the MCP surface in `packages/cli/src/mcp/`, and the
debugging doc.

The narrower half of this - a one-line dev warning when a view's
`overrideMaterial` skips instanced meshes, naming the view and the
count - landed in scene.ts (reported once per settle of the set) and
does not wait for the general read.
