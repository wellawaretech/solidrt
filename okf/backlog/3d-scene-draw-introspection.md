---
title: No way to ask why a mesh did not draw
description: Four independent mechanisms silently drop a mesh from a target - frustum culling, the layer mask, overrideMaterial skipping instanced meshes, and the shadow view's caster filter - and the scene exposes no introspection at all, so the only diagnosis is a screenshot and a guess; worse, the engine's per-entry switch can disagree with the node (a visible, in-view mesh left at instanceCount 0), and nothing can show that disagreement.
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

## When the engine's switch disagrees with the node

A fifth case is not a rule at all but a suspected engine bug, and it is the
one the read matters most for. In a streaming world (about 250 meshes,
reused as the window moves: re-pointed with `setGeometry`, moved with
`setTransform`, toggled with `setVisible`), some meshes were never drawn,
although they were placed, `mesh.visible` was true and they were in view.
Whole blocks of terrain stayed missing until the camera or the block moved
again. In `get_gpu_resources` their draw entries had `instanceCount` 0 and
an identity `uModel`: the matrix had never been written. The app never
writes either; only the spatial core's visibility and cull switch does.

What was observed around it:

- Turning engine culling off for those meshes
  (`setCulling(mesh, { frustumCulled: false })`) made the holes go away at
  once. Toggling it per mesh while leaving it on, in the same tick or a
  frame later, did not bring the missing ones back.
- With engine culling off and the app culling by box in JS (`setVisible`
  per block once a frame), the holes showed once more, and toggling the
  app's culling off and on cleared them.
- Headless tests of the paths a streamed slot takes (hidden-then-moved,
  late add, a three-stream voxel geometry with the real material, and the
  whole world streamed with culling re-tested) all pass. The trigger seems
  to need a live client's timing.

Places to look, none confirmed:

- `cull_pass` ([spatial/mod.rs](../../alloy/src/spatial/mod.rs)) writes an
  entry's count only when `want` differs from the sink's remembered
  `entry_on`, and the params only on the first turn-on of a fresh sink. If
  the entry is rebuilt or re-pointed without a rebind, or a test runs
  against a world box that is stale for one flush, the remembered state and
  the entry part ways and no later pass reconciles them.
- In the same pass, a write that does not land releases the sink
  (`return false` from `retain_mut`), silently and for good: that node
  never draws on that target again until something rebinds it.
- `setVisible` and `setCulling` return early when the JS-side value is
  unchanged (node.ts, mesh.ts), and the core's `set_visible` does the same
  against its own flag. A drift between any of these layers stays stuck,
  because the call that would repair it is skipped.

## Done looks like

A `scene.debugEntries()`-shaped read: one row per mesh, and per row the
targets it is drawn into plus the reason it is absent from the others -
visible / culled / layer-masked / override-skipped / not a caster. Dev
surface, allocation allowed: this is a debugging call, not a frame path.

Then the same data over MCP, so an agent can ask without editing the app -
`get_render_tree`'s counterpart for scenes, or a scene section on it.

For the drift case the rows have to carry the engine's side too, read back
from the core and not recomputed in JS (a JS recompute reports "visible"
for exactly the stuck entry): per sink, the core's `shown` and `entry_on`,
whether the sink is still bound, the frustum verdict and the world box it
tested, and the entry's actual instance count, next to the JS node's
`visible`. A row where they disagree is then named a stuck entry instead of
being inferred from `get_gpu_resources`.

Involves: `packages/3d/src/scene.ts` (the per-view entry walk already
computes the layer, override and caster reasons; the item is recording them
instead of returning early), the spatial core for visibility and culling
(`alloy/src/spatial/mod.rs`, a read of a node's sinks and their switch
state through `flux/src/alloy_plugins/spatial.rs`), the MCP surface in
`packages/cli/src/mcp/`, and the debugging doc.

The narrower half of this - a one-line dev warning when a view's
`overrideMaterial` skips instanced meshes, naming the view and the
count - landed in scene.ts (reported once per settle of the set) and
does not wait for the general read.
