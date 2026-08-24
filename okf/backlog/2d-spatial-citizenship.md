---
title: Sprites as spatial-core citizens
description: Sprite poses are JS-owned floats, so no core producer (native transitions, animation clips, physics) can ever move a sprite and picking is an O(n) JS walk; make each live sprite a spatial arena node whose InstanceRecord sink writes its instance-buffer slot, connecting 2d to the whole producer stack while rendering stays one instanced draw.
created: 2026-08-24
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

## Not in this item

Baked/tile layers ([2d-baked-layers](2d-baked-layers.md) stages A/B) are
untouched: tiles are static arithmetic, not nodes. Bitmap fonts, the frame
animation helper and atlas growth are orthogonal. Frustum-style visible-set
COMPACTION of instances (as opposed to per-entry 0/N visibility) is a
follow-on once a population needs it.
