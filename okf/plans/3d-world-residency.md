---
title: A 3d world larger than memory streams by the residency gate, with region handlers in core
description: The second handler over the spatial core's residency gate - a region as a scene node (explicit or a grid cell) whose content the app loads off the thread and builds under a per-frame millisecond slice, pooled geometry and textures, measured byte weights under the scene limit, a proxy drawn while unloaded (HLOD), streaming sources beyond cameras, and the handler lifecycle (load, abort, sliced build, unload, retain, state) in core so a 2d population handler is the same machinery one dimension down.
created: 2026-10-10
---

# A 3d world larger than memory streams by the residency gate

## Symptom

Frustum culling, occlusion and level of detail are core gates
(`packages/3d/AGENTS.md`, Culling and Level of detail): a node outside the
frustum, behind a wall or below its smallest level costs nothing per
frame. All three gate DRAWING. Everything still exists: geometry buffers,
textures and draw entries stay resident until the app disposes them. So a
world whose content does not fit in memory at once - an open map, a long
track, a city - has two options today, and both are the app's:

- Load it whole, bounded by device memory (the low-end Android tier sets
  that ceiling first).
- Stream by hand against `flux:spatial` directly: the gate decides
  (okf/done/spatial-residency-gate.md), but `createModel` bursts every
  create of a region into one frame, nothing is pooled, nothing is
  bounded in bytes, and nothing stands in for what is not loaded.

Three costs fall on every such app:

- **Streaming in hitches.** `createModel` runs synchronously on main, and
  buffer and texture creates are blocking calls to the raster thread
  ([3d-differentiators](../notes/3d-differentiators.md): cheap at steady
  state, expensive at churn). The 2d tile measurements put one chunk's
  create at 1-2 ms on a desktop GPU
  ([2d-tile-bulk-writes](../done/2d-tile-bulk-writes.md)); a region of
  models is many of those in one frame, with no way to spread them.
- **Churn instead of reuse.** Disposing and re-creating per approach pays
  the create cost every time; the pool exists
  (`createResourcePool`) but the 3d package's geometry and texture
  acquisition does not go through it.
- **A limit that counts cells.** The gate's limits weigh a cell by its
  grid's one `weight`, so a byte limit over a world whose cells differ a
  hundredfold (a desert cell, a city block) bounds nothing honestly.

## Three, Unity, Godot, Unreal

| | decision | loading | proxy | memory bound |
|---|---|---|---|---|
| Three | app | app, `dispose()` by hand | none (3DTilesRendererJS: LRU + SSE) | none |
| Unity | app | additive scenes, Addressables, `asyncUploadTimeSlice` (2 ms default) | LODGroup only | texture streaming budget |
| Godot | app | `load_threaded_request`; instancing on main is a burst | visibility ranges | none |
| Unreal | World Partition grid, streaming sources | level streaming, actor registration time-sliced per frame | HLOD per cell, cast shadows while unloaded | streaming pool with priority eviction |

Unreal is the one engine that ships all four, and its shapes are the ones
matched here: a grid cell is the unit, loading is time-sliced by
milliseconds not by count, the proxy is shown while the cell is unloaded,
memory is one pool with priority eviction, and streaming sources are not
only cameras. The gate did the decision and the eviction half; this item
is the handler half.

## Done looks like

**1. Region handlers in core, one lifecycle for 2d and 3d.** Load with
abort, the sliced build, cancel on leave, retain, the state machine and
the error policy are not 3d's: a 2d sprite-population handler needs the
same machinery. So `@solidrt/core` owns `createRegion` and
`createRegionGrid` as plain region handles over `flux:spatial`, driven by
the gate's two events, and each package adds what it owns: 3d the group
node per region and the cell placement, the builders and the measured
bytes; 2d the same over sprite layers later (the tile layer stays its own
synchronous handler and migrates when the population handler is written).

```ts
type RegionHandler<T> = {
  // Data prep: a file read, a decode, an isolate call. Async, abortable.
  load?: (region: Region, signal: AbortSignal) => T | Promise<T>
  // Creates, stepped under the slice budget (yield between creates).
  build: (region: Region, data: T) => void | Iterator<void>
  // The app's own state, BEFORE the content goes (an editable world
  // reads its cell back here).
  unload?: (region: Region, data: T) => void
  // Drawn while the region is not shown: HLOD.
  proxy?: (region: Region) => unknown
}
```

Lifecycle: enter runs `load` with an `AbortSignal`, then `build` as an
iterator stepped by the slice queue; the content is hidden while
building and shown when the iterator finishes, the proxy hidden with it.
`region.state` is `"unwanted" | "pending" | "stalled" | "loading" |
"building" | "shown"` (the first three read from the core). Leave is
immediate: a load in flight is aborted and its result discarded, a build
in progress cancelled and what it made torn down, `unload` runs first,
then the content leaves by remove plus dispose (never `destroy`: a
region leaves off screen, an exit transition would wait for nothing), the
proxy shows again. A `load` that throws is fail-soft: logged once, the
region wanted and empty, `onError` on the handle; the next enter retries.
`retain: n` keeps the data of the last n left regions (LRU) so a re-enter
skips `load`; a count, since `T` is the app's and only the app knows its
size.

**2. Time-sliced builds, a core primitive.** `@solidrt/core` gains one
frame-sliced work queue, Unreal's actor-registration time limit and
Unity's upload slice in one shape: `slice(steps, opts?)` returns
`{ cancel, done }`, `setSliceBudget(ms)` with a 2 ms default. The queue
drains at `frameStart`, FIFO (the gate hands enters out nearest first),
timing each step with the wall clock and stopping when the frame's spend
passes the budget. A step is whatever sits between two `yield`s, so a
blocking create inside it is measured as the frame actually paid it, on
any device.

**3. The 3d form.** `createRegion({ bounds, enter, leave, weight?,
priority?, retain?, label?, ...handler })` returns a group node you place
in the scene, its `bounds` the local box the gate tests;
`createRegionGrid({ origin, cell, enter, leave, bounds?, weight?,
limit?, priority?, retain?, label?, ...handler })` returns a root group
you add once, every resident cell a child region placed at the cell's
world corner so the content is cell-local (float precision per cell, as
the 2d chunk records are). `wrap` is refused for 3d grids: a 3d scene
has one world space and a wrapped world is a camera-teleport pattern in
every 3d engine. Components `<Region>` and `<RegionGrid>` take the same
props with `build` as a render prop. The package's builders yield at the
create boundaries: `buildModel(region, data)` (one texture or part per
step; `createModel` is its drained form, one upload path), `warmGeometry`
(pre-acquires a geometry's buffers so a later `add` finds them).

**4. Pooling inside the package.** One pool for the 3d package,
`setResourcePool(pool)` to share it with 2d layers, created lazily with a
constant cap at the first region. Once installed, geometry buffers and
model textures are taken from it on acquire (`writeBuffer` into a buffer
of the same byte class) and given back on release; pooled textures are
mutable textures by format, size and mip class, rewritten with
`uploadTexture`, which regenerates the chain. The pool hands out only
retired ids, so nothing writes into what a frame in flight reads.

**5. Limits in bytes, measured.** Core: explicit regions gain `weight`
and take part in limits and eviction like cells; a per-region measured
weight (`setRegionWeight`, `setCellWeight`) so the limit counts content,
the grid's `weight` staying the admission estimate; a per-grid
`priority` folded ahead of urgency in the enter order (a collision grid
before decorative scatter); `regionState` for introspection. The 3d
package reports a region's measured bytes when it shows (geometry under
it, its model textures, its record buffers). The 2d tile layer's weight
becomes bytes too, so one `setResidencyLimit` is bytes everywhere.

**6. HLOD as a proxy, not a gate.** The `proxy` node is drawn while the
region is not shown and hidden once it is, Unreal's rule: an ordinary
node, so it can be an LOD group of its own, and it casts shadows while
the content is unloaded. Shadow tiles keep counting as viewers (the
gate's rule stands); a region beyond its load distance that still casts
into view does so through its proxy. Residency stays a distance gate and
LOD a size gate. Hierarchical HLOD (a coarse grid whose cell content is
the proxy of many fine cells) is two grids with different margins plus an
additive `proxyCell` multiple later.

**7. Streaming sources.** `createStreamingSource({ position, radius })`:
a cube projection pushed as a viewer under `allocViewKey`, moved like a
node, so a fast vehicle wants the world ahead and a teleport loads its
destination before the cut.

**8. Legible, tested, documented.** `/stats` keeps the gate's figures and
the package registers `app.regionsShown`, `app.regionsLoading`,
`app.sliceMs`; region resources carry the grid's label so `/gpu` groups
are a footprint. Tests: `spatial_residency.rs` (weights, state,
priority, explicit eviction), a core test for the slice queue and the
handler lifecycle, `packages/3d/tests/residency.test.tsx` (both forms,
build stepping across frames, cancel mid-load and mid-build, the pool
round trip, the proxy swap, measured weights against a byte limit,
shown only when complete). `examples/streaming.tsx`: a procedural
terrain on a column grid, heightfields generated in a `"use isolate"`
module, instanced scatter per cell, a flat proxy per cell, a minimap
view, a byte limit that stalls the far cells, debug commands and the
control-API readback in the header. AGENTS.md Residency rewritten around
the package API; the gate's done note drops its "counts cells" limit.

Additive: a world that declares no region loads and draws exactly as now.

## Not in this item

- Mip streaming (Unity's texture streaming): resolution, not existence;
  it would share the byte limit and the pool.
- Data layers (Unreal): draw-layer masks on resident content, or a
  rebuild.
- A headless server simulating cells near players: same API, but the
  spatial core sits in alloy behind the gui feature (a placement
  question of the layering notes).
- Async creates ([gpu-async-compile-readback](../backlog/gpu-async-compile-readback.md)):
  the slice queue measures what a step costs, so it self-adjusts.

## Involves

`alloy/src/spatial/residency.rs` and `mod.rs`, `alloy/src/tests/spatial_residency.rs`,
`flux/src/alloy_plugins/spatial.rs` and `packages/flux-types/gui/spatial.d.ts`,
`packages/core/src/slice.ts` and `residency.ts` (+ tests),
`packages/3d/src/region.ts`, `model.ts` (the generator), `geometry-gpu.ts`
(the pool), `components/region.tsx`, `index.ts`, `AGENTS.md`,
`examples/streaming.tsx`, `tests/residency.test.tsx`,
`packages/2d/src/tiles.ts` (weight in bytes).

## Findings

Landed 2026-10-20 (uncommitted): the gate's weights and priority
(`residency.rs`, `RegionSpec`, `set_cell_weight`/`set_region_weight`,
`set_grid_limit`, `cell_state`/`region_state`, explicit regions under
the limits), the plugin and flux-types surface, core's `slice.ts` (the
frame slice queue over an installed driver) and `residency.ts` (the
lifecycle, both forms, retain, fail-soft), the 3d `region.ts` (the node
forms, `buildModel` over `modelSteps`, `warm`/`own`/`measure`,
`createStreamingSource`), `resource-pool.ts` with the pool in
`geometry-gpu.ts` and `model.ts` (decoded textures as mutable textures),
the components, the tile layer's weight in bytes, the tests
(`spatial_residency.rs` +3, `slice.test.ts`, `slice-frame.test.tsx`,
core `residency.test.tsx`, 3d `residency.test.tsx`), the example and
the docs.

- **The app test clock pins `performance.now()` at 0.** An app test has
  no wall clock, so a budget measured by it never cuts and a busy-wait
  hangs the frame (the harness fails the test after 30 s). The slice
  queue therefore runs on an installed DRIVER (clock plus frame
  request, `installSliceDriver`), pure like the pool, and its pacing is
  tested headless with a clock the steps advance by hand; under the app
  harness every step is free and a build drains in one frame, which the
  lifecycle tests are written for.
- **A viewer push alone demands no frame.** A camera write in a scene
  comes with a params write that does; a bare `setView` (a streaming
  source, a raw-gate test) and a region declaration do not, so the
  source's push and dispose, `attach` and a grid's `start` request one.
- **Cells behind the eye are wanted.** A load distance is a radius: a
  column grid under a camera wants the cells above and below the eye
  within `enter`. The tests bound their grids to one layer; the AGENTS
  text says so.
- **Block-compressed textures are create-once** by the GPU contract
  (`createMutableTexture`/`uploadTexture` refuse them), so the pool
  covers geometry buffers and decoded textures; a baked world's KTX2
  textures create fresh per enter. Lifting that is a flux/alloy GL
  change (compressed storage with per-level sub-uploads), filed as
  [gpu-compressed-mutable-textures](../backlog/gpu-compressed-mutable-textures.md).
- **The content shows in the frame its last step ran in**: the slice
  job settles synchronously inside the drain (`onSettle`), ahead of the
  frame's flush; the promise form settles after the frame.
- **An enter and its leave within one delivery collapse.** A scene's
  creation-time sync ran the gate with the default camera, the app's
  camera a flush later took it back, and the handler saw an enter and a
  leave in one batch: a load started to be aborted. `take_events` now
  drops such pairs (budget refunded, counters corrected), the gate's
  "a pending region the camera turned away from never enters" rule
  extended across one window.
- **Cells behind the eye are wanted by design**: a load distance is a
  radius around the view volume (Unreal's streaming sources), so the
  world is there when the camera turns. The stacking the tests hit came
  from finite cells on the up axis; a one-layer world is the column
  grid, 0 on the up axis.

