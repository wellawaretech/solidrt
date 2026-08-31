---
title: Sprites as spatial-core citizens
description: Sprite poses are JS-owned floats, so no core producer (native transitions, animation clips, physics) can ever move a sprite and picking is an O(n) JS walk; make each live sprite a spatial arena node whose InstanceRecord sink writes its instance-buffer slot, connecting 2d to the whole producer stack while rendering stays one instanced draw.
created: 2026-08-24
completed: 2026-08-31
---

# Sprites as spatial-core citizens

## Symptom

A sprite's pose lives in 13 JS-owned floats published through the buffer
lease. That design is optimal for exactly one workload - JS computing every
position every frame - and locks the package out of everything being built
around the spatial arena:

- **No core producer can reach a sprite.** Native transitions on node
  transforms ([spatial-core](spatial-core.md) lists them),
  [animation-core](animation-core.md) clip sampling and
  [physics-core](physics-core.md) bodies all write node-local TRS into the
  arena. Core cannot animate floats it does not own, so tier-2 sprites are
  permanently excluded from the producer stack: every moving sprite needs a
  JS `onFrame` loop even when the motion is a spring, a clip or ballistics.
  Tier 1 (`d-texture`) has shipped native transitions; tier 2 lost them.
- **Picking is an O(n) reverse walk in JS** per pointer move, and every
  live record draws whether or not the camera sees it - the two
  O(population) costs [2d-baked-layers](2d-baked-layers.md) planned to fix
  with a hand-rolled JS uniform grid.
- **The layer is flat.** A dragged stack, a ship with turrets, a panning
  group: all composition is app JS, recomputed per child per move in the
  interpreter, while the arena computes moved subtrees in Rust.

The reusability argument is the sharp end: core blocks exist to be
foundational. The `InstanceRecord` sink row in spatial-core's table
("instanced fleets whose instances are nodes: the thousands-of-dynamic-
objects tier") is the one missing bridge between the producer stack and
instanced 2d rendering - build it and a 2D physics game with zero per-frame
JS falls out of composition, nothing 2d-specific added. Dynamic games are
not the exception to citizenship; they are its strongest consumer, because
they have the most per-frame-per-entity work to push below the interpreter
line.

The package is brand new and uncommitted-adjacent: the window to re-found
the live layer is now, before the API hardens.

## Shape

Rendering does not change. The layer stays one instanced draw into one
pipeline target, composited as a SINGLE `<texture>` leaf; sprites never
become rendertree elements. What changes is who owns the pose upstream of
the instance buffer:

- A live sprite is a **spatial arena node** (arena slot: local TRS, parent,
  world matrix, BVH leaf, sink binding - no layout, no paint, no element).
  Node scale is the sprite's w/h: every sprite is a scaled unit quad.
- An **`InstanceRecord` sink with a 2D pose projection** writes the node's
  world pose (x, y, angle, sx, sy) into that sprite's slot of the instance
  buffer at flush. Projections are established sink vocabulary (SharedSlot
  has Direction; Position anticipated); this passes the admissibility rule:
  input is the flush's output, destination an existing engine channel, no
  domain name in core.
- **Style fields (uv frame, tint) stay JS-owned.** A sink that interprets
  or packs non-transform data fails the admissibility rule by design, so
  the record splits: a spatial instance buffer core writes, a style buffer
  JS writes.
- **The raw records path survives as the escape hatch** for motion only JS
  can compute (bespoke flocking, per-frame gameplay logic over every
  entity). It is not the game tier; it shrinks as producers land. The axis
  of the tier model is not retained-vs-dynamic but WHERE MOTION IS
  COMPUTED: core producers vs a JS per-frame loop.

## What it brings

- Every producer, present and future, reaches sprites for free: native
  node transitions (retarget once, zero JS and zero FFI per frame),
  animation-core tracks (2D clips are just TRS tracks), physics-core
  bodies bound to nodes. Without citizenship each block needs a parallel
  2d integration or simply cannot reach sprites.
- Hierarchy with moved-subtree-proportional flush cost, in Rust.
- Picking and culling from the core BVH (flat boxes already cost correctly
  in the SAH - spatial-core findings). A shared unit-quad SHAPE gives
  exact rotated-rect narrowphase through the existing Moller-Trumbore
  path, and `raycast` already returns `uv`, so alpha-accurate picking
  against the atlas is reachable later. The JS uniform grid planned in
  2d-baked-layers becomes unnecessary for the live layer.
- Softens [2d-sprite-sort-key](2d-sprite-sort-key.md): fixed instance
  slots plus depth-tested, alpha-tested sprites make draw order irrelevant
  for opaque-or-transparent pixel art, the overwhelming case.
- The sink itself is a reusable-block investment: it serves 3d instanced
  fleets identically, per its existing table row.

## What core needs (ranked by size)

1. **`InstanceRecord` sink + 2D pose projection** - world matrix to
   `[x, y, angle, sx, sy]` slots. Designed in the sink table, never built.
2. **Batched core-side instance publish.** Sinks today write draw params
   per changed node; this sink wants the flush to accumulate slot writes
   into a staging copy and publish dirty ranges once. Load-bearing, not
   nice-to-have: the common dynamic case is thousands of nodes moved per
   frame by core producers with no JS in the loop, so the whole path from
   producer step to instance buffer must stay allocation-free and batched.
   Design it in from the start, not retrofitted.
3. **Per-attribute instance buffers.** The pipeline API takes one
   `instanceBuffer`; the spatial/style split needs two (core-owned pose,
   JS-owned style). Co-writing one interleaved buffer from two owners is
   an ordering hazard. GL binds multiple VBOs trivially.
4. **Rect/point overlap queries** on the BVH - already listed as a future
   consumer in spatial-core; small.
5. **Native transitions on node transforms** - its own item,
   [spatial-node-transitions](spatial-node-transitions.md) (the smaller
   sibling of animation-core), and the LINCHPIN, see below.

## The honest cost

A core transform write measured ~7 us (spatial-core bench: 0.07 ms for 10).
JS-driven per-frame motion routed through nodes is therefore ~7 ms at 1k
sprites versus ~0.13 ms through the records array - citizenship WITHOUT
producers is a motion regression that buys only hierarchy and picking. The
value inverts the moment native node transitions (then clips, then physics)
land: retained and produced motion costs zero JS and zero FFI per frame.
Sequencing follows: this item lands with or after native node transitions,
and the records path stays for genuine JS-computed swarms either way.

## Findings

Decisions 2026-08-24: build order is citizenship plumbing first, native
node transitions immediately after (the records path stays untouched
throughout, so nothing regresses while the tier is incomplete); and
`addSprite` itself becomes node-backed when the package rewires - the
node tier is the live layer, not a parallel opt-in, with `records` +
`touch()` remaining the raw escape hatch.

Core stage (item 1 + 2 of the list, the sink and the batched publish)
landed 2026-08-24 (uncommitted):

- `alloy/src/spatial/mod.rs`: `InstanceProjection::Pose2D` (world matrix
  -> `[x, y, angle, sx, sy]`; angle is `atan2(m[1], m[0])`, sy negates
  when the 2x2 determinant is negative so mirroring survives),
  `InstanceRecordSink { buffer, index, projection }`, and per-buffer
  `InstanceGroup` staging mirrors. The flush stages slot writes and
  publishes ONE coalesced dirty range per buffer through the new
  `SinkWriter::write_instances(buffer, first, values)` - however many
  nodes moved, one buffer write per flush. A staged write equal to the
  slot's current floats publishes nothing.
- Slot lifecycle: hidden node = zeroed slot (zero scale collapses the
  instance; per-instance visibility without touching the draw's count),
  unhide rewrites even when the matrix never changed (`record_on` flag),
  unbind/rebind/destroy zero the abandoned slot, groups refcount and
  drop with their last write. One stride per buffer (projection
  mismatch errors at bind).
- `Context::spatial_bind_record` validates at bind time (buffer exists,
  slot fits its byte size); the writer maps `write_instances` onto the
  existing `write_gpu_buffer` partial write (one `WriteBuffer` raster
  cmd + `note_buffer_content`, so target dependency propagation sees it).
- `flux:spatial` gains `bindPoseRecord(node, buffer, index)` /
  `unbindRecord(node)` (projection named in the function like
  `bindDirectionSlot`); flux-types updated.
- Tests in `alloy/src/tests/spatial.rs`: decomposition incl. mirroring,
  coalescing across sparse slots (unbound gap slots ship as zeros -
  fine, the buffer is wholly core-owned), no-op suppression, hide/show,
  unbind/destroy zeroing, group drop.

Live verification waits for the package rewiring stage: binding needs a
GPU buffer, which a headless flux check cannot create. Remaining: item 4
(overlap queries), the package rewiring, then
[spatial-node-transitions](spatial-node-transitions.md).

Growth swap settled and landed 2026-08-24 (uncommitted): sinks store the
raw buffer id, so a doubled pose buffer would have cost one rebind FFI
call per node. `retargetRecords(old, new)` on `flux:spatial`
(`Spatial::retarget_records` + `records_extent`) moves the staging group
and every sink in one call and marks the whole used range dirty - the
next flush republishes everything into the new buffer as one bulk write,
slot indices untouched. Layer growth flow: create zeroed doubled buffer
-> retargetRecords -> setDraw `instanceBuffers` swap -> destroy old.
Validated at the call site (source has records, destination exists, fits
every bound slot, and does not already carry records - checked in that
order); old==new is a no-op; the freed old id may later carry a fresh
independent group. Considered and rejected: a logical group-id
indirection (a new id space for one rare event) and accepting the O(N)
rebind hitch.

Per-attribute instance buffers (item 3) landed 2026-08-24 (uncommitted),
as instance buffer SLOTS - the pipeline/entry split forbids buffer ids in
the pipeline desc, so the layout side declares slots and the entry binds
one buffer per slot (WebGPU's vertex-buffer-layout model):

- JS surface, additive: an instance attribute takes `slot` (default 0,
  slots dense from 0, at most `MAX_INSTANCE_SLOTS` = 4); the entry passes
  `instanceBuffers: [a, b]` (or the existing `instanceBuffer` for the
  single-slot case - both together throw). `setDraw` buffers update:
  `instanceBuffer` swaps slot 0, `instanceBuffers` swaps all slots and
  must fill exactly the slots the entry fills. flux-types + core gpu.ts
  updated; existing single-slot callers (layer.ts, tiles.ts, scene.ts)
  unchanged.
- alloy: `PipelineDesc.instance_attributes` is (name, format, slot);
  `DrawSpec`/`BufferIds` carry `instance_buffers: [u64; 4]` (fixed array
  keeps Copy); `DrawBounds.instances` is per-slot (stride, size) with
  `instance_limit()` = the tightest slot, so the DERIVED INSTANCE COUNT
  IS THE MIN CAPACITY ACROSS SLOTS and bounds errors cite the limiting
  slot. VAO build binds each slot's buffer and records its group at
  divisor 1 (divisor still VAO state, pass.rs untouched). Introspection:
  `instance_buffer_ids` list; attributes carry `slot`; the go connection
  emits `instanceBuffer` (single) / `instanceBuffers` (plural) and
  `slot` only off default.
- Verified: alloy/examples/draw_instanced.rs grew a two-slot section
  (split fetch, tightest-slot derivation, slot-1 write re-renders, full
  swap, density + missing-slot errors) - all pass on real GL; flux
  gpu_split.rs likewise (slot render + both-keys / gap throws),
  GPU-SPLIT-OK; alloy tests 271 green; srt check green.
- Traps: `SDL_VIDEO_DRIVER=offscreen` runs the alloy/flux examples with
  NO window (Mesa EGL pbuffer) - always use it, a bare run opens a
  window on the user's desktop. Both example files were STALE from
  earlier committed API changes (draw_instanced.rs predated the
  program-coverage check, so its plain pipeline needed its own
  pos-only program; gpu_split.rs predated positional params, so every
  create passed opts in the params slot) - fixed in passing; an example
  panic in the app.run closure leaves the SDL loop polling forever, so
  a "hung" example usually means a failed assertion.

Overlap queries (item 4) and the package re-founding landed 2026-08-24
(uncommitted):

- Core: `Bvh::query` (box walk over the fat boxes) + `Spatial::overlap`
  with an exact narrowphase - `pick::box_overlap` runs separating axes of
  both boxes (the three world axes and the three transformed local axes,
  unnormalized so scale and shear stay valid), so a rotated flat rect
  tests exactly, never by its world AABB; only genuinely 3D edge-edge
  poses can err conservative. `overlap(bounds)` on `flux:spatial` returns
  unordered node ids; reads the index as of the last flush like raycast.
- `@solidrt/2d` re-founded: `addSprite` creates a spatial arena node
  (scale = w/h, unit-quad bounds `[-0.5,-0.5,0, 0.5,0.5,0]`), binds its
  Pose2D record into the POSE buffer (instance slot 0, core-written);
  style `[uv frame, tint]` is a second JS-written buffer in slot 1.
  Sprites hold FIXED slots (free-list recycle; draw order = slot order,
  no painter-order guarantee across removals). Growth doubles both
  buffers with one `retargetRecords` + one `setDraw({ instanceBuffers })`.
  `pick` = core raycast at [x, y, -1] along +z (the local-box test IS the
  exact rotated-rect, no Shape needed), topmost = highest slot, filtered
  to the layer's own nodes (the arena is shared with 3d scenes);
  `pickRect` = the overlap query. Hierarchy: `addGroup`/`setGroup`/
  `setSpriteParent` + a `<Group>` component - groups are plain nodes
  (x, y, rotation, UNIFORM scale); sprites never parent sprites, because
  a sprite node's scale IS its pixel size and would multiply into
  children. The records path moved whole to records.ts as
  `createRecordLayer` (13-float layout, records + touch(), JS pick walk) -
  it cannot share the node layer's buffers because the core staging
  mirror owns the pose buffer and republishes gap slots as zeros. One
  sprite-function surface (addSprite/setSprite/getSprite/removeSprite)
  dispatches over both layer kinds via internal layer methods.
- Verified live (examples/parity.tsx, srt run + release go client): the
  same 200-sprite population through both layers is PIXEL-IDENTICAL
  (0/129600 off after the pose round-trips the core decomposition), pick
  agrees at 200 random points, pickRect exact on the left-half marquee,
  group rotation carries children, slots recycle. Bench there: addSprite
  x200 11.5ms nodes vs 7.3ms records; move-all x200 2.6ms vs 1.3ms - the
  known ~7us-per-transform-write gap that native node transitions (the
  linchpin, next) invert.
- Traps: the dev client is dist/linux-x64-gnu/solidrt-go and STALE dist
  binaries fail with pre-slot errors ("declares instanceAttributes but
  no instance buffer") - `make client` after alloy/flux GPU-surface
  changes before srt-run verification. The node layer must
  `spatial.flush()` in dispose BEFORE destroying its pose buffer, or the
  core's final slot-zeroing writes land on a dead buffer id and warn.
  readTexture-comparing the two layer outputs in-app is the strongest
  parity check and needs no snapshot plumbing.

The linchpin landed 2026-08-24: native node transitions
([done](../done/spatial-node-transitions.md)) invert the motion cost -
setSpriteTransition makes setSprite writes targets, a 400-sprite retarget
burst is ~4 ms once per target change instead of ~5 ms per frame
(examples/springs.tsx is the living bench). The motion story stands
without the regression caveat.

## Not in this item

Baked/tile layers ([2d-baked-layers](2d-baked-layers.md) stages A/B) are
untouched: tiles are static arithmetic, not nodes. Bitmap fonts, the frame
animation helper and atlas growth are orthogonal. Frustum-style visible-set
COMPACTION of instances (as opposed to per-entry 0/N visibility) is a
follow-on once a population needs it.
