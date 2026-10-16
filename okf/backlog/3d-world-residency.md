---
title: A 3d world larger than memory has no residency, so every streaming app hand-rolls loading, freeing and pooling
description: Culling and LOD gate drawing, not memory, so an open map or a long track either loads whole or streams by hand from onFrame (camera distance in JS, createModel and dispose, or a pool re-pointed with setGeometry), and with createModel synchronous on main and GPU creates blocking, every region that streams in costs a frame; the 2d tile layer's stage B2 is the same problem one dimension down.
created: 2026-10-10
---

# A 3d world larger than memory has no residency

## Symptom

Frustum culling and level of detail are core gates (`packages/3d/AGENTS.md`,
Culling and Level of detail): a node outside the frustum, or below its
smallest level, costs nothing per frame. Both gate DRAWING. Everything
still exists: geometry buffers, textures and draw entries stay resident
until the app disposes them (`disposeGeometry`, `model.dispose`,
`disposeInstances`; AGENTS.md, Node lifecycle). So a world whose content
does not fit in memory at once - an open map, a long track, a city - has
two options today, and both are the app's:

- Load it whole, bounded by device memory (the low-end Android tier sets
  that ceiling first).
- Stream by hand: read the camera in `onFrame`, decide which regions are
  near, `loadModel`/`createModel` the ones coming in and `dispose` the ones
  going out, with hysteresis of its own so a camera on a border does not
  thrash.

The second shape exists, with pooling bolted on: about 250 meshes reused
as the window moves, re-pointed with `setGeometry`, moved with
`setTransform`, toggled with `setVisible` - the setting of the suspected
cull-switch bug in
[3d-scene-draw-introspection](3d-scene-draw-introspection.md). The
chunk-streamed forest that was the demand evidence for instancing
([3d-roadmap](../notes/3d-roadmap.md), item 12) is the same pattern for
populations.

Three costs fall on every such app:

- **The decision runs in JS every frame.** The core already holds every
  target's frustum and a BVH over the scene (`alloy/src/spatial/`), the
  inputs a residency decision needs; the app recomputes distances in the
  interpreter instead, and a still camera still pays.
- **Streaming in hitches.** `createModel` runs synchronously on main, and
  buffer and texture creates are blocking calls to the raster thread
  ([3d-differentiators](../notes/3d-differentiators.md): cheap at steady
  state, expensive at churn). The 2d tile measurements put one chunk's
  create at 1-2 ms on a desktop GPU
  ([2d-tile-bulk-writes](../done/2d-tile-bulk-writes.md)); a region of
  models is many of those in one frame, with no way to spread them.
- **Churn instead of reuse.** Disposing and re-creating per approach pays
  the create cost every time; the pool that avoids it is app code,
  re-pointing meshes through paths the engine does not test as a
  streaming workload.

## Three, Unity, Godot

- **Three**: nothing built in. Apps load and `dispose()` by hand; tiled
  streaming lives in libraries (3DTilesRendererJS: screen-space-error
  refinement over an LRU cache).
- **Unity**: additive scene loading (`SceneManager.LoadSceneAsync` with
  `LoadSceneMode.Additive`, `UnloadSceneAsync`) and Addressables for
  reference-counted asset residency; uploads are time-sliced
  (`QualitySettings.asyncUploadTimeSlice`), and mipmap streaming runs under
  a memory budget. Which regions to load is still the game's code (or
  Entities sub-scene sections).
- **Godot**: background loading (`ResourceLoader.load_threaded_request`)
  and visibility ranges for HLOD-style fades; world streaming is left to
  addons.

None of the three ships the residency DECISION as a primitive; Unity ships
the hitch-free loading under it. The decision is where we can do better:
it has the shape of our LOD gate (a per-target spatial test in Rust,
events only on change, nothing per frame for a still camera), and none of
the three has that gate in the engine.

## Done looks like

- **A residency gate in the spatial core**, beside culling and LOD: a
  region is a node with bounds and a load distance (with hysteresis); a
  camera move re-tests regions in Rust against every target's camera and
  reports enter and leave to JS only on change. The app's handler decides
  what a region holds - a model, procedural geometry, instance records -
  so the policy stays the app's and the decision stops being per-frame JS.
- **Loading that does not hitch**: a region's creates spread over frames
  under a per-frame budget, the region shown when complete (a coarse proxy
  or nothing until then). Compiles lean on
  [gpu-async-compile-readback](gpu-async-compile-readback.md); parsing
  runs in an isolate, already the documented shape for runtime glTF.
- **Pooling as the engine's**: a freed region's buffers and meshes go to a
  free list the next region takes, instead of dispose-and-create per
  approach - the rule 2d stage B2 states for chunks.
- **No write into what a frame in flight reads.** A model upload, a
  buffer write or a resize into a resource the last one or two presented
  frames still read makes a tiled driver drain its queue first: 27 ms per
  write on an Adreno 610, the hitch of
  [2d-tile-world-frame-hitches](2d-tile-world-frame-hitches.md). The pool
  (`createResourcePool` in @solidrt/core/gpu) hands out only resources
  the GPU is done with (released at a `submittedFrames` count that
  `retiredFrames`, published from the raster thread's present fences,
  has reached), and a region that changes while
  resident takes a retired set and swaps, as the tile layer's re-bake
  does, instead of writing in place.
- A worked example walking a world several times larger than what is
  resident at once, the resident set and frame times read back through
  the control API.

To settle while shaping:

- **One primitive for 2d and 3d.** Settled 2026-10-13, specified in
  [spatial-residency-gate](../done/spatial-residency-gate.md): one gate, two
  region forms, both for both packages. EXPLICIT regions (the node with
  bounds and a load distance above: a level section, a 2d room) and a
  GRID region set, the form an unbounded or procedural world needs
  because its cells do not exist until approached; Unreal's World
  Partition and Minecraft's chunk columns are 2d grids streaming 3d
  content, so the grid form is not 2d's. The gate decides and paces (one
  per-frame enter budget shared by every consumer in the scene, nearest
  first), the pool is shared, and a HANDLER acts: the tile layer is a
  built-in handler because its content is engine-owned (the bake, the
  records); a 3d region's handler is the app's because its content is
  the app's models. That asymmetry is in who owns the content, not in the
  mechanism, and the mechanism is built once. This item is the second
  handler over the gate, after 2d stage B2.
- **HLOD.** [3d-lod](../done/3d-lod.md) left HLOD out deliberately. A
  region whose far level is a merged proxy and whose near level is the
  loaded content is residency and LOD as one gate; decide whether the gate
  is an LOD level that loads, or its own.
- **Several targets.** Split-screen, a minimap view, shadow tiles: a
  region is resident if any target wants it, presumably, with shadow
  casters counted.

Additive: a region is a new node kind or flag, and a world that declares
none loads and draws exactly as now.

## Involves

`alloy/src/spatial/` (the gate, beside `cull.rs` and the LOD select in
`mod.rs`), the flux spatial plugin for the enter and leave events,
`packages/3d` (the region API, pooling, the budgeted create path) and
AGENTS.md (a Streaming section beside Culling and Level of detail), plus
whatever 2d stage B2 takes from the same gate.
