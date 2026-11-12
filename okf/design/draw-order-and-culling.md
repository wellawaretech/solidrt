---
title: Draw order and culling
description: How the core orders what a target draws - the entry order between draw entries (3d's queues), the instance order inside one entry (splats, ordered record populations), and view culling of a population - measured against how Unity and Godot separate storage, visibility and order; the decision that a population the app writes is drawn per view from a draw list built over its visible set, never from a buffer kept in cull or key order; facts in alloy, the culling default in lattice; and damage, a write to a culled view's drawn record re-rendering only its boxes. Read before touching gpu/order.rs, gpu/damage.rs, context/order.rs, the spatial order pass or feed, or a 2d layer's orderBy or cells.
created: 2026-11-07
---

# Draw order and culling

Ordering and culling were built in four steps over two months: the
entry sort for 3d scenes, the instance order for 2d `orderBy`, the index
stream and sort thread for splats, and cell culling for sprite layers.
Each step reused the one before, and the last one reused the instance
order as its culling structure. That coupling made a record write cost
the whole population (14 to 15 ms of JS thread per frame on the
tablet), made culling change paint order, and forbade `cells` beside any
order but y. Each fix was then shaped as a patch on the coupling and
spawned the next item. This document is the picture, kept current: the
model engines use, the two ordering levels, and the decisions that
replace the coupling.

The code this describes: `alloy/src/spatial/mod.rs` (`order_pass`,
`draw_key`, `DrawQueue`, `DrawOrder`, `set_draw_sort`, the order feed
`order_feed_pass`), `alloy/src/gpu/order.rs` (the key vocabulary, the
radix sort, run tables, cells), `alloy/src/gpu/order_worker.rs` (the
sort thread), `alloy/src/context/order.rs` (the registry, the mirrors,
the publish hooks, the cell readers), `packages/3d/src/scene.ts` (queues
and `renderOrder`), `packages/3d/src/mesh.ts` and `splat.ts` (the
indexed order and the hand-off), `packages/2d/src/records.ts`,
`layer.ts` and `views.ts` (`orderBy`, `cells`, the views), and for
damage `alloy/src/gpu/damage.rs` (`ListDamage`) and
`alloy/src/raster/targets.rs` (`flush_dirty_within`).

## Vocabulary

- **Draw entry.** One draw on a target: a pipeline, its buffers, a
  range. A 3d mesh is an entry; a 2d layer has one entry per view.
- **Population.** The instance records of an entry: sprites, record
  meshes, splats. A record lives in a **slot**; slot i keeps meaning
  record i for the record's life.
- **Entry order.** The order of the entries on one target.
- **Instance order.** The order of the records inside one entry.
- **Key.** What an order sorts by: a field (y, `renderOrder`, an app
  float), a projected position (view depth), a cell.
- **Mirror.** The core's slot-order copy of a population's records,
  kept by `context/order.rs`.
- **View.** A camera over a target. The spatial core feeds an entry its
  view as the node's model-to-clip matrix (the order feed, or the cull
  feed on every change for a culled entry). It is the matrix the entry's
  vertex stage projects with (D8).
- **Visible set.** The records of a population whose boxes a view
  meets.
- **Draw list.** A view's records to draw, in draw order: the visible
  set, ordered by the key if there is one, slot order if not.

## The reference model: Unity and Godot

Both keep three things apart:

1. **Storage.** A sprite's data stays with the sprite. A write costs
   that sprite.
2. **Visibility.** Every frame, each object is tested by its bounds
   against the camera: Unity natively over flat bounds arrays, Godot per
   canvas item rect in its canvas walk.
3. **Order.** Every frame, the draw set is sorted from scratch. Unity
   sorts the culled renderers by sorting layer, order in layer, then
   distance or a custom axis (its 2d y-sort is the custom axis). Godot
   sorts a y-sort node's children with a comparison sort. The batch is
   rebuilt from the sorted list.

Neither keeps a draw order between frames, neither sorts
incrementally, and neither lays a dynamic population out by cell. The
per-frame cost follows what is drawn; a frame where nothing changed
does nothing. Chunk layouts (Unity's Tilemap chunk mode, Godot's
rendering quadrants, Unreal's HISM) are for content that rarely moves;
Unreal warns that HISM instance updates rebuild its tree, and chunk
mode gives up order across chunks. Godot's answer for very large counts
is MultiMesh, which neither sorts nor culls per instance.

## The two levels

### Entry order (3d)

`Spatial::order_pass`, on a target with `set_draw_sort` on (a 3d
scene; off for shadow tiles, override views and 2d targets). The key
(`draw_key`): the queue (background, opaque, cutout, transparent), then
`renderOrder`, then depth - a log bucket of distance for opaque and
cutout (front to back), exact forward depth for transparent (back to
front) - and bind order breaks ties. Two gates keep it cheap: a camera
move under `ORDER_BUCKET_MARGIN` of the nearest bucketed center
re-keys nothing when no entry is transparent, and the permutation is
written only when it differs from the last. Tens of entries, a
comparison sort. This is the Three and Unity model and stays.

Between 2d layers there is no entry order: each view is its own
target, and layers stack in rendertree order.

### Instance order

One primitive in `gpu/order.rs`: the key vocabulary (`OrderKey`:
`Field`, `Projected`, `Cell`; `PackedKey` for splats), a stable LSD
radix sort (11-bit digits, three passes, allocation-free in steady
state), and two materializations of one permutation: the records
gathered into key order, or an index stream of u32 record ids into a
data texture. Culling of a population is a `RunTable`: boxed
contiguous spans a view's matrix selects.

Its consumers today:

| | 2d layer `orderBy` / `cells` | 3d record mesh | Splats |
|---|---|---|---|
| Key | `Field` (y, `renderOrder`, app float) or `Cell` | `Field`, or `Projected` with `retain` | `Projected`, packed unorm16x3, descending |
| Records | the app writes them (lease or spatial record sinks) | the app writes them | handed off once, fixed |
| Re-sorts on | every publish | every publish; a projected key also on a view turn past `ORDER_DIRECTION_EPS_COS` | the view gates only |
| Writes | the whole gathered prefix of every instance buffer | the whole gathered prefix | the index stream (4 bytes per record) |
| Thread | UI | UI | the sort thread above `SYNC_SORT_MAX_KEYS` |
| Culling | cells: the buffer kept in cell order, spans per view | none | packed runs, thinned by the LOD ramp |

The splat column fits the primitive: the records never change, so the
order is a function of the view, and the index stream makes a re-sort
cost 4 bytes per record. The other two columns do not: their key lives
in records the app writes, so every write may move a key, and the
primitive answers with a full sort, a full gather and a full upload.
Under `cells` the buffer order is also the cull structure, which is
where the 2d symptoms come from (D2).

## Decisions

### D1. Entry order and instance order are separate levels

The entry order sorts tens of entries by queue, `renderOrder` and
depth; the instance order sorts thousands to millions of records by a
key. They share the idea of a gated, change-only write and nothing
else.

### D2. The buffer layout is never the cull structure

Superseded: "Contiguous ranges, not an index stream ... no gathered
copy per view" in
[2d-sprite-view-culling](../done/2d-sprite-view-culling.md). That item
cited chunk culling (Unity's Tilemap, Godot's quadrants, Unreal's
HISM), which is for static content, and applied it to dynamic sprites.
Keeping a population's buffer in cell order ties storage, visibility
and order into one physical layout, and each consequence followed from
it:

- A write costs the population: a byte range has no fixed place in a
  permuted buffer, so every publish re-gathers the prefix (14 to 15 ms
  p50 per writing frame against 1 ms unordered, 30,703 records on the
  Adreno 610 tablet).
- `cells` forbids `renderOrder` and `{ field }`: the cell order and the
  draw order must be the same order.
- `cells` without `orderBy` changes paint order across cell
  boundaries: culling decides what draws over what.
- Every keyed publish re-selects every reader, and sibling buffers
  republish to stay coherent with the layout.

Rejected with it: the fix the backlog item first proposed - an inverse
permutation mapping dirty slots to gathered positions, an incremental
re-sort that must reproduce the stable order exactly. It keeps the
coupling and adds state to maintain it.

### D3. A population the app writes draws per view from a draw list

For a population with culling or an order on, each view's entry draws
its own draw list:

1. **Storage.** Records stay in slot order in the mirror. A write is a
   dirty range into the mirror, as on an unordered layer: no gather, no
   upload at the write.
2. **Visibility.** Cells are a membership index over the residency
   lattice: a slot list per cell, updated when a written record changes
   cell. Under the view's matrix, the cells the view meets give the
   candidates; in a cell the view's edge crosses, each record's own box
   decides (the boxes the mirror already keeps, with the `minScreenPx`
   reach).
3. **Order.** Without a key, the visible slots are taken in slot order
   (a bit per slot, scanned in order), so the paint order is exactly the
   unculled one. With a key, the visible slots are radix-sorted by it,
   stable on slot order.
4. **Materialize.** The visible records of every instance buffer of the
   entry are gathered into the view entry's own instance buffers, and
   the entry draws `[0, visible)` as a plain instanced draw. No spans,
   no binding at an offset.

The list is rebuilt for a view on a frame where its view changed, a
written slot joined or left its drawn set, or a drawn slot's key
changed. A write that does none of that patches the list: the written
records are re-gathered at their list positions, the list kept (a
slot-to-position map and the keys of the last build, `DrawList::change`),
as one span from the first changed position through the last and one
buffer write per instance buffer; a buffer write costs per write on a
tiler, not per byte (Measuring). Otherwise nothing runs. The cost of a
rebuild follows the visible set: about 650 records on the tablet board
instead of 30,703; the cost of a patch follows the write's span.

| | write | frame with a change | frame without |
|---|---|---|---|
| unordered, not culled (`cull: false`) | the dirty range uploaded | the target re-renders in full | nothing |
| culled and/or ordered, drawn set and keys kept | the dirty range into the mirror | per view: the written drawn records re-gathered in place; a culled view's target re-renders inside their boxes (D8), an ordered one in full | nothing |
| culled and/or ordered, view moved or drawn set or key changed | the dirty range into the mirror | per view: cull, sort, gather of the visible set; the target re-renders in full after a view move, inside the written records' boxes otherwise when culled (D8) | nothing |
| ordered, everything visible, keys moving | the dirty range into the mirror | per view: sort and gather of the population; the target in full | nothing |

The last row is what every engine pays for a fully visible sorted set
(Godot re-sorts all y-sort children every frame); culling is what makes
it follow the screen. A view that shows the world (a minimap) gathers
the world; that is its honest cost.

### D4. Which form a population takes follows who writes its records

- **The app writes the records** (2d layers, 3d record meshes with a
  field key or a retained projected key): the draw list (D3). A
  projected key there is re-sorted under the same direction gate the
  feed already applies.
- **The records are handed off and fixed** (splats, any
  `transferRecords` population): the view-triggered instance order -
  the index stream, the sort thread, packed runs and the LOD ramp. Its
  order is a function of the view alone, so it is kept between
  re-sorts and never re-gathered.
- **Neither culled nor ordered**: the entry draws the layer's buffers
  directly, as today.

### D5. Facts in alloy, the culling default in the layer's API

Alloy offers culling and an order as opt-ins per entry and carries
them out; it chooses neither. It reports facts per view: the
population, the visible count, the draw-list rebuilds and their cost.

Whether a layer culls when the app did not say is the default of the
layer's API, as Three's `frustumCulled = true` is `Object3D`'s: a fixed
value that depends on no runtime fact, so it is not a runtime policy.
The API declares it to alloy explicitly; alloy never culls unasked.
An app's `cull: false` opts a layer out. The 2d sprite layer culls by
default, as every engine culls its sprites; the record layer does not
(`cull: true` opts in), as Godot's MultiMesh and Three's InstancedMesh
do not cull per instance: its use is a JS loop rewriting every record
each frame, where culling saves nothing while the records are in view
and costs its per-record work (measured,
[per-view-draw-lists notes](../notes/per-view-draw-lists.md)).

Superseded: the culling default as lattice's policy, with a population
threshold. No engine culls by population size (Unity, Godot and Unreal
cull every object; Three by default, with a per-object opt-out), and a
threshold needs a live switch between direct drawing and a list as a
layer grows. Lattice owns a culling policy only once one depends on
runtime facts (alloy's visible and population counts per view); that
is additive, because an absent option keeps meaning the default.

### D6. Culling never changes what draws over what

A culled view draws exactly what the unculled view draws, minus what is
off screen. Every order combines with culling; no option throws for the
combination.

### D7. Picking follows draw order

On an ordered layer, the hits of a pick are ordered by the same key,
stable on slot order: the order the draw list uses, so the topmost hit
is the sprite drawn on top. The hit set is small; no view's draw list
is needed. The record layer reads the key from its own records. The
node layer reads `renderOrder` from its sprite records and, for the y
key, each hit's world y from the core (`worldMatrix`), the pose its key
buffer holds.

### D8. A write to a drawn record costs its box, not the target

A culled view's target re-renders only where a write can have changed
it. At the rebuild point the list knows the slots the write touched;
each one's cull box as it was at the last point and as it is now,
through the list's view, is a rectangle of the target. The target's dirt
is those rectangles, and the raster thread renders the target through
the partial path inside them alone: each wiped (color and depth) and the
whole entry list redrawn under its scissor, every other pixel kept. The
cost of a sparse write's frame follows the records' boxes. On the
Adreno 610 a damage render of one sprite's box costs 0.59 to 0.71 ms
where the full pass of the same target costs 16.7 to 35.0 ms, with no
large fixed cost for the no-clear pass
([target-damage-measurements](../notes/target-damage-measurements.md)).
A patch, a rebuild and a population change through the instance count
are alike: a record that moved, changed key, joined or left the drawn
set changes only its own box.

It renders in full whenever no rectangle can be trusted: a change of
the view, the viewport, or the list's sort key or direction, a list that
does not cull (no boxes: `cull: false`, or an ordered list alone), more
than 64 touched
slots, a box with a corner at or behind the eye, rectangles over half
the target, a multisampled target, a pass without its wipe program, any
dirtying that is not the list's own write (a params write, an entry
change, a re-rendered source), and once, at its first change, a target
holding a draw over the slicing floor (65,536 vertex-instances) that the
submission budget has not measured yet, so the full render measures it. Rectangles merge where they meet and collapse to their bounding
rectangle past 4. A list on a sub-target and a parent with tiles take the
same model ([gpu-target-damage-draw-lists](../done/gpu-target-damage-draw-lists.md)).

The window follows the target. The content change the frame takes
carries the same dirt, and a texture node showing the target damages the
window only where it shows the rectangles: each through the node's fit,
source rectangle and window map
([gpu-target-damage-window](../done/gpu-target-damage-window.md)).
Under Recording boundaries the map is composed up the boundaries'
frames, so a moved or scrolled boundary damages where the node shows
now. It is the whole node (the outermost one, under one) under a
snapshot boundary or under a view with a blur filter, for a target with
a mip chain, and under a map that is not 2d. A test checks the window's
pixels: a headless frame under the patch switch patches the window
target as a window patches its back buffer, through the rig or through
the patch layer, and the bytes must match a whole redraw. The window is
patched from that damage on a single-sample backbuffer (every desktop
platform) through the rig, and on a multisampled one (every Android
device) through the patch layer where the display offers
`EGL_KHR_partial_update`: the patch is declared as the frame's damage
region, drawn into the corner of the patch layer (one layer kept between
frames, at least the stepped patch's size, bounded: far larger than the
patch for a run of frames, it is allocated again at the patch's size)
and copied into the window by a copy scissored to the patch. A backdrop
panel the damage touches pulls the panel and the blur's reach past it
into the patch. The window damage is one rectangle,
the bounding box of all that changed in the frame: two sprites that
change in opposite corners repaint nearly the whole window.
On a desktop client a press on a full-window view damages 5,214 px a
frame of 921,600.

Two contracts carry it, and nothing checks either:

- **The view is the vertex stage's matrix.** The view a culling list is
  fed is the matrix its records are drawn with, to within a pixel. A
  frustum test is symmetric in y, so culling cannot tell a mirrored
  matrix from the right one; damage can, and a mirrored rectangle wipes
  rows the write never touched and leaves the changed ones stale. The 2d
  views fed a y-up projection beside y-down vertex stages until damage
  showed it; `viewMatrices` is y-down now and `tests/camera.test.ts`
  pins it to the stage. The gated order feed lags small turns and moves,
  so it must never drive a culling list; 3d feeds only order lists, which
  carry no cells.
- **The cull box bounds what a record draws.** Culling took that trust
  already: a vertex stage that moves geometry past its material's
  declared `cullMargin` is culled early at the view's edge. Under damage
  the same mistake shows anywhere in the view, as stale pixels where the
  geometry was or now is outside the box. The force-full switch
  (`/force-full-renders`, the MCP `set_force_full_renders`, reported as
  `forceFullRenders` in `/gpu`) diagnoses it: stale
  pixels that go away under it name a wrong rectangle. Alloy reports
  per target how it rendered (`fullRenders`, `damageRenders`,
  `damagedPixels` in `/gpu`).

Rejected: a coarse cell grid to merge rectangles into. A ring-shaped
reveal is a few hundred writes, past the slot cap and full anyway, and
four merged rectangles cover a sparse write.

## Known limits and open items

- A cull box is the record's rect, grown by its screen-size reach and
  by the material's `cullMargin` (cells `margin`, Godot's
  `extra_cull_margin`): a vertex stage that moves geometry further than
  its declared margin is culled early at a view's edge, and leaves stale
  pixels anywhere in the view after a write (D8). An unbounded move
  wants `cull: false`.
- Damage (D8) redraws the target's whole entry list once per rectangle,
  so its vertex work multiplies by the rectangle count (at most 4). A
  per-rectangle sub-list of the records that meet it, queried from the
  cells and drawn at an instance offset, is the better form; alloy has
  no instance-offset draw. A layer drawn directly stays full; damage
  declared by an app is shaped and has no consumer:
  [gpu-target-damage](../backlog/gpu-target-damage.md).
- [2d-sprite-layer-view-relative-records](../backlog/2d-sprite-layer-view-relative-records.md):
  views of one sprite layer far apart share one f32 origin. Decided: a
  list's gather writes each drawn record's position relative to its own
  view's target origin, from f64 positions the record sink stages in
  the list mirror (camera-relative rendering), in place of records
  chunked per region cell. A `cull: false` layer keeps one origin.
  Record layers write f32 records themselves, so it does not help them.
- [3d-instance-additive](../backlog/3d-instance-additive.md): its
  per-instance frustum gating is the draw list (D3) on a 3d instanced
  mesh.
- Several views over one ordered population each sort and gather their
  own visible set. Shadow views draw unordered: a depth-only pass gives
  the same result in any order.
- [splat-order-per-view](../backlog/splat-order-per-view.md): an
  indexed population (splats) has one id stream, sorted for the scene's
  camera, which every other camera view reads as it is.

## Measuring

The board of record: the 30,703-record hull board on the Adreno 610
tablet (Galaxy Tab A7). Check the CPU governor and thermal status
before every run and state them. Two scenarios, both required:

- **The pan.** A scripted 3 s touch drag: GPU per frame, the part
  outside the pass, instances at the end (`/gpu`), frames in 3 s.
- **The reveal.** The board's first reveal animation, a few hundred
  record writes per frame: the JS-thread frame time at p50 and p95
  (`/stats`), and the draw-list rebuild point's time (`drawListMs`,
  sampled mid-run), which runs on the UI thread outside the JS figure.
- **The churn.** The whole board in view and every record moved each
  frame: a flock's workload at the board's size, the worst case for
  culling (it saves nothing and pays its per-record work).
- **The tint.** The layer tint pulses each frame and nothing else: a
  re-render with no write and no upload, the draw's own GPU cost. On the
  Adreno 610 the per-pass timer and the frame-level GPU figure disagree
  about culled draws (the pass reads about 1.2 ms higher, the frame 1 to
  1.5 ms lower); on a GPU that does not bin (Intel Mesa) the culled
  pass reads 2.5 times lower than the unculled one and the frames agree.
  The pass timer on the Adreno attributes the work, it does not total
  it: compare culled and unculled there by the frame-level figure.

Buffer writes into an instance buffer the GPU still reads cost per
write on the Adreno 610, not per byte: a draw list patch of 28 writes
a frame drew a third fewer frames than one write of nearly twice the
records. A patch is one write per buffer (D3).

Each with `cull: false`, `orderBy: "y"` unculled, culled (the default),
and culled with `orderBy: "y"`; frames over the whole scenario beside
every figure, since the JS figure alone hides the rebuild point. Under
culling a frame that changes nothing on screen is not drawn, so a
culled scenario's frames count the frames that showed a change, not
throughput. A change in this
area is done when every scenario is measured, not one. The probe and
its bench: `probes/record-layer-order-probe.tsx`,
`probes/record-layer-order-bench.py`; the results:
[per-view-draw-lists notes](../notes/per-view-draw-lists.md).

## Records this gathers

- [2d-baked-layers](../done/2d-baked-layers.md): the 2026-09-06 decision
  that an off-screen record costs nothing worth culling, overturned.
- [2d-sprite-vertex-early-out](../done/2d-sprite-vertex-early-out.md):
  zero-size slots skip the vertex stage.
- [2d-sprite-view-culling](../done/2d-sprite-view-culling.md): cells as a
  buffer order, its measurements (pan 41.7 to 36.0 ms GPU, reveal 14 to
  15 ms JS), superseded by D2 and D3.
- [per-view-draw-lists](../done/per-view-draw-lists.md): D2 to D7 built,
  measured on the tablet board (notes:
  [per-view-draw-lists](../notes/per-view-draw-lists.md)).
- [splats](splats.md): the index stream, the sort thread, packed runs.
- [gpu-target-damage-draw-lists](../done/gpu-target-damage-draw-lists.md):
  D8 built, damage rectangles fed by the draw lists, and the 2d view
  matrix made the vertex stage's.
