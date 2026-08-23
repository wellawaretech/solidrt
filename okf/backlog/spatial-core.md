---
title: Spatial core - transform hierarchy, spatial index and queries in alloy
description: The @solidrt/3d sync walk recurses the whole node tree in QuickJS on every change (one moved node = O(scene)), picking is a JS box-only test, and both are the interpreter-hostile parts of every large scene. Move the transform hierarchy, its flush and the spatial index into a generic alloy module (no camera, no mesh, no lights) that the 3d package is the first consumer of; triangle-accurate picking (3d roadmap item 4) and the scene-walk descent (item 19) land together on it.
created: 2026-08-23
---

# Spatial core

## Symptom

`packages/3d/src/scene.ts` `sync()` walks the ENTIRE node tree every time
anything is dirty: `walk(root)` recurses into every child whether or not its
subtree changed, so a single `setTransform` costs O(scene) interpreted
work. At ~10 us per visited node that is the budget-killer for any scene
with more than a few hundred nodes, static or not. Picking is in the same
place: a JS AABB tree (`bvh.ts`) as broadphase and a box test as the
narrowphase, so hits carry no `face`/`uv`, concave meshes report their box,
and the per-triangle tier cannot be written in JS at all (O(vertices) per
query, the documented interpreter rule).

Both are instances of the same thing: O(scene) or O(vertices) work running
at rung 1 of the escape ladder ([3d-differentiators](../notes/3d-differentiators.md)).
The draw-list design was chosen so this work can move to core "without an
app-facing API change" ([scene-graph-3d](../notes/scene-graph-3d.md),
"Scale and the interpreter"); this item is that move.

## Shape: generic, not a "3d scene"

What moves is not a 3D scene graph. It is the part every spatial consumer
shares:

- a **transform hierarchy**: nodes with local position/quaternion/scale, a
  parent, visibility, a cached world matrix, and dirty propagation that
  recomputes only dirty subtrees;
- a **flush** that recomputes what changed and hands the results to sinks;
- a **spatial index** over node world bounds (the AABB tree `bvh.ts` is
  today), with queries;
- **sinks**: where a node's fresh world matrix goes. A sink never changes
  the walk, it only consumes its result.

The admissibility rule for a sink, stated once because every row must
pass it: (1) its INPUT is exactly the flush's output - a node's world
transform, nothing else; (2) its DESTINATION is an existing engine
channel addressed by caller-supplied ids and names (a draw entry's
params, its range, a target's shared params); (3) no domain constant or
name lives in core - no `uLightDir`, no light/bone/instance concept.
(`uModel`/`uNormal` in the DrawParams sink are the sanctioned exception:
they are the draw list's own standard vocabulary, engine-level, not a
package's.) Consumer count is not the test - shape is: a second consumer
must need zero core changes. A sink that would need core to INTERPRET
the transform (fold an intensity, pack colors) fails the rule and stays
in the consumer.

Nothing in the module knows about cameras, perspective, lights, materials
or meshes. A consumer supplies matrices (a view-projection is target
state, written through the existing shared-params channel, not through
this module) and attaches sinks and bounds to nodes. `@solidrt/3d` becomes
the JS face over it - components, materials, generators, camera, lights,
pointer routing - the relationship `packages/core` has to the rendertree.

The sink table, of which only the first row is built here; the rest is the
reason the binding is a small enum and not a hardwired mesh field:

| Sink | Writes the world matrix to | Consumer |
|---|---|---|
| `DrawParams { target, draw, normal }` | the draw entry's `uModel` (+ `uNormal`) params | `@solidrt/3d`; any draw-list user (2D sprite scenes with an orthographic matrix are the same thing) |
| `InstanceRecord { buffer, index }` | one slot of an instance buffer | instanced fleets whose instances are nodes: the thousands-of-dynamic-objects tier |
| `TextureSlot { texture, index }` | one row of a float texture | skeleton bones for skinning (3d roadmap item 16) |
| `EntryVisible { target, draw }` | the entry's instance count (0 / N) | frustum culling (item 19's other half) |
| `SharedSlot { target, name, len, index, projection }` | one vec3 slot of a target shared array param (Direction projection today; Position is the anticipated sibling) | `@solidrt/3d` light directions (`uLightDir`); any tracked axis a shader reads |

Other consumers of the same index, none built here: overlap and sphere-cast
queries for the lightweight collision tier games need without a physics
engine; emitter world positions for spatial audio; native transitions on
node transforms, the spatial analogue of the 2D tree's shipped native
transitions.

## Placement and rules

`alloy/src/spatial/`, main thread, beside `Context`. It issues the same
`RasterCmd`s the JS walk issues today (`UpdateDrawParams`, `UpdateDrawRange`,
`SetDrawOrder`); nothing on the raster thread changes and no GL is touched,
so every platform follows for free. Rendertree rules apply: engine
independent, native Rust types in and out, JS marshalling only in
`flux/src/alloy_plugins/spatial.rs`, `flux-types` parity.

Ownership split with the JS side (decided 2026-08-23): JS keeps the LOCAL
transform as the source of truth - `node.position` etc. stay plain readable
arrays, no FFI on a read - and `setTransform` forwards the write to core.
World state (world matrices, bounds, index) lives ONLY in core;
`worldPosition`/`lookAt`/`project` read it back through one call. The
alternative (core owns everything, JS reads through FFI) is cleaner but
makes every read a crossing.

## Stage 1 - the walk

- Node arena: create/destroy, set parent (reparent allowed), set local
  TRS, set visible. Dirty flags on local and on world; a set marks the
  subtree world-dirty without visiting it (the visit is the flush's job,
  and it stops at clean subtrees).
- `DrawParams` sink, one per node: on a fresh world matrix the flush writes
  `uModel` and, when asked, `uNormal` (the inverse-transpose the JS
  `normalMatrix` computes now); on a visibility change it flips the
  entry's instance count (0 / the node's "on" count, which an instanced
  mesh sets to its record count).
- `flush()` called once per microtask by the JS scheduler, exactly where
  `sync()` runs today; returns nothing. Visible-state and world matrices
  are readable per node for the JS-side transparent sort and light params,
  which stay in JS this stage (they touch a handful of nodes).
- `@solidrt/3d` rewired with no app-facing change: `_local`, `_world`,
  `compose`, `multiply`, the recursive walk and `updateLeaf` leave
  `scene.ts`; `worldInto` becomes a core read.
- Done looks like: the scene bench (a few thousand nodes, one moving) shows
  the flush cost proportional to the moved subtree, not the scene; every
  `packages/3d/examples/` renders unchanged.

## Stage 2 - index and queries (3d roadmap item 4)

- The AABB tree moves in: a node with local bounds gets a leaf, refitted by
  the flush from the fresh world matrix (the exact `updateLeaf`
  construction), hidden nodes kept in the tree and skipped at query time.
- Opt-in triangle data per node: `setTriangles(node, positions, stride,
  offset, indices)` registers CPU copies (memory only for what is picked or
  collided against). Narrowphase is Moller-Trumbore over the candidates
  the tree returns; a per-geometry triangle BVH only if a real model shows
  it is needed.
- `raycast(origin, dir)` returns `(node, t, point, normal, face, uv)`; box
  fallback for nodes without triangles. Hits gain `face`/`uv`, concave
  silhouettes become correct, `bvh.ts` is deleted. Pointer capture/hover
  bookkeeping stays in JS. (Built as shapes per geometry rather than
  `setTriangles` per node - see the findings.)

## Not in this item

Frustum culling, the `InstanceRecord` and `TextureSlot` sinks, overlap
queries, transitions on nodes: each is its own item once a consumer exists,
and each is small on top of this. Lights and the transparent sort stay in
JS until node counts make them matter.

## Findings

Stage 1 landed 2026-08-23 (uncommitted): `alloy/src/spatial/`,
`Context::spatial*`, `flux:spatial`, `scene.ts` rewired, unit tests in
`alloy/src/tests/spatial.rs`, bench `probes/scene-walk-bench.tsx`. All
3d examples verified live (scene-basic, lit, pick with real taps,
instanced).

- Bench, 3000 static meshes + one group of 10 moving (release client):
  core flush 0.03 ms/frame; the transform write 0.07 ms. The old JS walk
  visited all 3010 nodes per frame (not re-measured; at the ~10 us per
  visited node of the signal-path figures that is tens of ms). With the static
  count at 300 the flush is the same 0.03 ms - proportional to the moved
  subtree, as required.
- The remaining per-frame JS cost is the picking leaf refit: ~0.15-0.3 ms
  per MOVED mesh (10 moved = 1.5-3.5 ms), independent of the scene size.
  It runs only when something picks or sorts in that frame, and stage 2
  removes it entirely (leaves refit in core from the flush). Two details
  for stage 2: the JS tree has no rotations, so inserting a grid in row
  order degenerates its depth (shuffled insertion halved the refit cost);
  and a small fat margin (`MARGIN` of the largest extent) re-inserts a
  rotating mesh every frame.
- Transparent sort: which meshes moved is the core's knowledge now, so
  any move with two or more transparent meshes marks the order dirty
  (the sort issues nothing when the permutation is unchanged). That is
  already the industry baseline - Three.js sorts its transparent list
  every frame unconditionally - and the cost only shows for a static
  camera plus opaque-only motion. Three escalating fixes, none demanded
  yet:
  1. JS subtree flag: each scene node tracks "subtree contains a
     transparent mesh" (maintained at attach/detach/reparent); a move
     dirties the order only if the moved node's flag is set or the
     camera moved. Small, JS-only, exact for the common shapes.
  2. Core dirty bit: mark nodes sort-relevant in core, flush() reports
     whether any marked node's world changed. Exact under reparenting
     too, JS stops maintaining flags. Passes the sink admissibility
     rule (pure flush output, no domain names).
  3. Core transparent sort: a view-depth sort as a sink writing the
     draw-order channel. Also removes the per-frame center readbacks,
     but core would need the view matrix - hold for real demand.
- Node lifetime: core nodes exist only while the JS node is in a scene
  (created at add, freed at remove/dispose), so nothing needs a
  finalizer; outside a scene `worldInto` composes the chain in JS.

Stage 2 landed 2026-08-23 (uncommitted): `alloy/src/spatial/bvh.rs`
(the JS tree ported), `pick.rs` (shapes + Moller-Trumbore), `setBounds`/
`createShape`/`setShape`/`raycast` on `flux:spatial`; `bvh.ts` deleted,
`rayBoxDistance` moved to `math.ts`, the JS differential rig replaced by
`index_matches_linear_oracle` in the Rust tests. Verified live: pick
example taps (hit points now on the sphere surface), lit, bench.

- Shapes are per GEOMETRY, not per node, and every ordinary mesh gets
  one (created with the geometry's GPU buffers, same refcount): the
  memory is one xyz+uv+index copy per distinct geometry, which is the
  data `geometry.vertices` already holds in JS. A per-node opt-in was
  not worth an API: the cost is bounded by distinct geometry, not scene
  size. Instanced meshes stay box-only (records are opaque).
- Bench after stage 2 (3000 static + 10 moving): JS sync 3.3 ms -> 0.03
  ms, core flush 0.04 ms INCLUDING the ten leaf refits. A hitting
  raycast through the row-order grid was 0.37 ms until SAH rotations
  landed (below); with them it is ~0.004 ms.
- Trap: `Spatial` derives Default and the tree's empty root is -1, so
  `Bvh` needs a hand-written Default - a derived one (root 0) makes the
  first leaf its own sibling and the tree loops.
- The `Bvh` is not 3D-specific: flat (2D) boxes cost via their other
  faces in the surface-area heuristic, so a 2D sprite scene or 2D
  overlap queries use it unchanged.

Follow-ups landed 2026-08-23 (uncommitted):

- SAH rotations (Box2D lineage: child/grandchild swaps along both refit
  walks) in `bvh.rs`: a 3025-leaf grid inserted in row order stays <= 40
  deep (test `ordered_insertion_stays_shallow`), and the bench's hitting
  raycast fell 0.37 ms -> 0.004 ms. The oracle test pins correctness.
- The `SharedSlot` sink (Direction projection): `uLightDir` is now fully
  core-driven - each directional light's slot follows its node's world
  rotation with `-direction` as the local vector, so a light that merely
  moves (or rides a rotating group) costs no JS at all, and the
  light-ancestor-moved check plus the world-matrix readback left
  `scene.ts`. JS still owns colors, count, hemisphere and slot indices
  (`writeLights`, attach/detach/field changes only). Slots zero on
  unbind/destroy; groups are refcounted and dropped with their last
  write; `scene.dispose` drains the zeroed slots with one `flush()`
  before destroying the target so nothing writes to a dead target.
- `fillAttribute(geometry, name, fill, first?, count?)` and
  `fillColors(geometry, fill, first?, count?)` take the Geometry and read
  `.layout` from it, like every sibling helper (the positional
  `VertexLayout | undefined` param was the fix for two long-standing
  type errors in `checks/geometry-check.ts`, then judged ugly and
  replaced; breaking for out-of-repo demos, like the layout work). The
  raw-array form survives as the private `fillSlot` under withAttribute;
  the pure layout helpers spell the parameter `layout?: VertexLayout`.
- The `SinkWrite` enum became the `SinkWriter` trait (write_params /
  write_count / write_shared), defined in `spatial/mod.rs`: the core's
  entire output contract in one place, borrowed arguments (no per-write
  String/Vec allocation on the shared path), implemented by a small
  `Writer` in `alloy/src/context/spatial.rs` (resolves against the draw
  mirrors, warns on a stale binding) and by a test recorder.
  `set_sink_count` writes through the trait too and returns whether it
  wrote. Part of the same pass: `context.rs` split into `context/` by
  concern (mirror/texture/buffer/program/target/capture/spatial/content),
  file names pairing with `gpu/`'s, and the texture registry + sampler
  vocabulary moved from `src/texture.rs` to `gpu/texture.rs` (imports are
  `crate::gpu::*` now; `alloy::` root re-exports unchanged).
- `raycast` on `flux:spatial` takes `(origin, direction)` as two
  Float32Arrays of 3, closing the plugin's arg-style rule: every
  vector-shaped payload travels as one Float32Array (transform, bounds,
  slot vector, ray); scalars only for genuine scalars (ids, counts,
  offsets). The rquickjs ~8-arg cap forces this shape anyway once a call
  carries ids plus options.

Considered and rejected (2026-08-23): collapsing the picking shape into a
retained `createBuffer` CPU copy. The ledger says no: a shape stores only
positions + uvs, deinterleaved (5 of the standard layout's 8 floats per
vertex, cache-friendly for the narrowphase), while a retained buffer copy
would hold the full interleaved data - MORE memory, not less - and the
raster thread drops its upload Vec today, so retention would be a new
cost, not a reuse. It would also couple spatial to the gpu buffer
registry (lifetime entanglement across modules) where shapes are
currently self-contained. The JS-side `geometry.vertices` is app-owned
plain data the engine cannot free. If shape memory ever shows up, the
lever is LAZY shapes - create them on first pointer handler / raycast
per scene instead of at every buffer acquire - not buffer references.
