---
title: Depth ids, scene views and shadow maps
description: What is true about sampleable depth, per-target sinks, scene views and the shadow set regardless of how they were built - cut from the shadow-maps plan's Findings on close.
created: 2026-08-27
---

# Depth ids, scene views and shadow maps

The durable findings of [3d-shadow-maps](../done/3d-shadow-maps.md),
kept here because they govern code that outlives the plan. The plan
file keeps the archaeology (what was decided, flipped, or deferred).

## Depth ids (engine)

- Impeller adopts a `DEPTH_COMPONENT24` GL texture through
  `adopt_opengl_texture` without complaint (the descriptor claims RGBA8;
  Impeller only binds and samples), and a display-list
  `draw_texture_rect` of it comes out as `(d, 0, 0, 1)` on the Linux GL
  path - ES 3.0's depth-texture sampling rule, red channel only. So a
  depth id is displayable via `<texture src>` for free, as a red-tinted
  depth view, provided the draw samples NEAREST: the id's fixed
  `SamplerState::DEPTH` makes the paint walk pick nearest, which keeps the
  texture complete. The registry needs no optional Impeller handle and
  the paint walk no special case.
- A depth id is an alias, not a texture, everywhere a graph question is
  asked: the UI-side sampler mirror records the OWNER target for a binding
  to it (`Context::source_of`), and the raster flush graph maps binding
  sources through `depth_owners`. Recording the raw depth id instead
  would leave content propagation, cycle detection and reclamation blind
  to the edge, since none of them index by anything but target ids.
- Ownership follows the color exactly: adopted name, Impeller deletes it
  on handle drop, a resize allocates a FRESH depth name and re-adopts at
  the same id (respecifying the old name would race in-flight display
  lists and, worse, the old handle's drop would delete the live texture).
- The three fused creates (`create_shader_texture`,
  `create_pipeline_texture`, `create_shader_target`) validate their
  initial bindings on their own (unit budget only) and never pass through
  `validate_new_bindings`; a per-binding rule added there alone (the
  linear-override-on-depth rejection) silently misses them. Any binding
  rule must be applied at both places, or the creates routed through the
  shared validator (a small refactor waiting for a second rule).
- An alloy example panicking inside `app.run`'s closure (the srt-ui
  thread) leaves the main thread pumping the SDL window: a black window
  that never closes. `depth_texture.rs` installs a panic hook that exits;
  the other examples do not.

## Per-target sinks (spatial core)

- `entry_on`/`fresh` stay OFF the public `DrawSink` (the caller's bind
  spec, Copy, compared in tests) and live in a private `BoundSink {
  sink, entry_on, fresh }` on the node - the flush state is the core's,
  not the binder's. Sinks and slots are keyed by target only (one entry
  per mesh per target is the 3d package's invariant; a (target, name)
  key for slots is the additive widening if a node ever feeds two arrays
  of one target). `uNormal` is computed once per node per flush however
  many sinks ask.
- A rebind (bind on a target the node already draws into) re-queues the
  node, and a queued node recomputes as changed (the reparent rule), so
  the node's OTHER sinks get a params rewrite in that flush.
  Pre-existing for the single sink (`set_bounds` does the same); rebinds
  are rebuildEntry-rare. Splitting "queued for structure" from "queued
  for bookkeeping" is the fix if it ever shows in a profile.

## Scene views (library)

- An `overrideMaterial` view draws in add order - no renderOrder, no
  transparent sort. The sort reads `mesh._transparent` (the mesh's own
  material), which says nothing about the override; for a depth pass
  order is irrelevant, and a transparent override is a visualizer's
  problem. `orderEntries` takes an `entry` accessor so a non-overridden
  view sorts with its own camera's view matrix and its own per-mesh
  entries; the world-space centers refresh once per sync for every sort.
- `pick()` handles `ortho` (rays along the camera forward axis from a
  point on the camera plane); `project()` under ortho has w = 1
  everywhere, so its behind-the-camera null never fires there. A view
  has no pick.
- The light set is rewritten for EVERY target whenever it changes or a
  view is created (`writeLights` rebinds each light's direction slot per
  target - the slot re-seeds at the flush); the merged `scene.setParams`
  names are kept in `sceneParams` and replayed on a new view, because a
  view created after a `setParams({ uTime })` would otherwise never see
  the name. `view.setParams` is the view's own channel and is not
  replayed anywhere.

## The shadow set (library)

- The y orientation of target-to-target depth sampling needs no
  correction on the GL path: the receiver looks up with the very matrix
  that rendered the map and raw texture coordinates (`ndc * 0.5 + 0.5`),
  and GL stores the map in the same convention it rasterized it in, so
  the flip cancels. Verified on ANGLE/D3D11 as well (2026-08-27): the
  same self-consistency holds, no platform branch in the lookup.
- Every receiving target carries all MAX_LIGHTS `uShadowMap<i>`
  bindings at all times - a 1x1 white placeholder (depth 1, never
  shadowed) for a slot that does not cast, the light's depth id
  otherwise - so the no-shadow state is deterministic rather than
  "whatever unit 0 holds" (the engine does accept a declared-but-unbound
  sampler at entry creation). Shadow views are excluded from the binding
  (same-pass feedback).
- The shadow slot IS the directional light index: `uShadowMap0..3`,
  `uShadowMatrix[4]`, `uShadowCast[4]`, `uShadowBias[4]`,
  `uShadowNormalBias[4]` (`SHADOW_SLOTS`), no light-to-shadow mapping
  array. The maps are separate sampler uniforms picked by an if-chain
  (`SHADOW_LOOKUP`) because GLSL ES 3.00 only indexes sampler arrays by
  constant expressions. `writeLights` owns the whole slot set (casts,
  biases, the sampler binds), so a reorder or a detach re-slots
  everything in one rewrite and a new view is seeded by the same
  `lightsDirty`. The matrices are ONE 64-float `uShadowMatrix` param
  (the engine writes whole arrays), identity in non-casting slots,
  rewritten when any shadow camera is pending or the slots changed. A
  receiving program binds four sampler units (placeholders included),
  `uMap` making five, against a minimum of sixteen.
- A shadow camera is re-placed by comparing the light's world matrix
  against the one it was last placed from (16 compares per shadow per
  sync), not by scanning the `moved` list for the light or its
  ancestors; a core-driven transition on the light is followed only when
  some sync runs, since a transition alone schedules none - the same
  limit picking's sort keys have.
- `_shadowChanged` on a size change resizes the shadow view in place
  (`setTargetSize`; the depth id survives), so `setLight(light, {
  shadow: { mapSize } })` never rebinds the map.
