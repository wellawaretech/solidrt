---
title: 2d spatial queries - overlap, sweep, raycast and move-and-slide for sprites
description: Every live sprite already sits in the core's picking index, but the 2d package exposes only pick and pickRect, so a circle query, a cast along a motion or a move-and-slide has no path while the 3d scene wraps all of them with filters and a character mover on top.
created: 2026-09-06
---

# 2d spatial queries

## Symptom

`@solidrt/2d` answers two questions about the world: which sprites lie
under a point (`pick`) and which touch an axis-aligned rect (`pickRect`,
the marquee). A 2d game asks more:

- Which sprites does this circle touch (an explosion radius, a pickup
  range, a melee arc)?
- Does this body reach anything when it moves by this delta, and where
  does it first touch (a bullet, a dash)?
- Move this body by this delta and slide along whatever it hits (the
  platformer and top-down character mover).

None of these has a path today. Apps rebuild them in JS over `getSprite`
walks, which is exactly the O(n) loop that
[2d-spatial-citizenship](../done/2d-spatial-citizenship.md) removed from
picking. Meanwhile the 3d scene exposes `raycast(origin, direction,
opts?)`, `overlap(volume, opts?)`, `sweep(volume, motion, opts?)` with
`QueryOptions` filters, and `moveAndSlide` on top
([spatial-collision-queries](../done/spatial-collision-queries.md)). The
core behind both packages is the same arena, the same BVH and the same
narrowphase: `pickRect` is already a `spatial.overlap("box", ...)` call
in layer.ts, and the core's capsule with `a == b` is a circle.

## Shape: the 3d contract, one dimension down

On the node-backed sprite layer (the record layer has no index and stays
JS-walked):

- `layer.raycast(x, y, dx, dy)`: every shown sprite the ray strikes in
  the layer plane, nearest first, as `{ sprite, distance, point }`. `pick`
  stays as the point form.
- `layer.overlap(volume, opts?)`: `{ sprite, point, normal, depth }` per
  touched sprite, unordered. Volumes are 2d spellings of the core's:
  `Circle { x, y, radius }`, `Capsule { ax, ay, bx, by, radius }` and
  `Rect { x, y, width, height, rotation? }` (the 3d `OrientedBox`). Each
  packs to the core volume with a unit z extent so every sprite plane
  lies inside it, the way pickRect does now. `pickRect` becomes
  `overlap` over an unrotated rect and stays as the marquee convenience.
- `layer.sweep(volume, dx, dy, opts?)`: `{ sprite, time, point, normal }`
  earliest first, the core's sweep with the motion in the plane.
- `QueryOptions`: `sprites?: Sprite[]` as the include-list (3d's
  `meshes`); the `layers` bitmask joins when
  [2d-layer-views](2d-layer-views.md) brings it to sprites.
- `moveAndSlide(layer, volume, dx, dy, opts?)`: the 3d mover's shape with
  2d vectors - `up` defaults to `[0, -1]` (y-down), `floorMaxAngle`,
  `maxSlides`, `skin`, `floorSnap`, and the result's `floor`, `wall`,
  `ceiling` and `hits`. Whether this is a port of collision.ts or the 3d
  mover generalized over its vector type is an implementation decision;
  the algorithm is identical.

Results name the sprite, as pick does, never a node id. Every query runs
the layer's pending batch first (the write-then-query coherence pick
has) and filters to the layer's own nodes (the arena is shared with 3d
scenes).

Comparison: Godot's `PhysicsDirectSpaceState2D.intersect_shape` /
`cast_motion` and `CharacterBody2D.move_and_slide` are the same trio;
Unity's `Physics2D.OverlapCircle` / `CircleCast` / `BoxCast` and the
`Rigidbody2D` mover likewise. Three has no 2d, so the 2d spelling
follows Godot's names where the 3d package followed Unity's (the verbs
overlap and sweep are already fixed by the 3d surface).

## Done looks like

A probe drives a capsule character across a floor of sprites with a wall
and a slope: `moveAndSlide` walks it up the slope, stops at the wall with
`wall` true, and reports `floor` on the ground; an `overlap` circle finds
the sprites within a blast radius; a `sweep` along a shot reports the
first sprite and the touch point. The example's marquee is `overlap` over
a rect and returns what `pickRect` returned.

## Not in this item

Tile-layer collision: tiles are baked records, not nodes, so a tile world
needs its own collision surface (a solid-cell grid query) rather than the
sprite index. Physics-core producers moving sprites
(okf/backlog/physics-core.md) are the layer above this one.

## Done 2026-09-07

Shipped as shaped, with three decisions the implementation forced:

- The filter moved into the core. Every query used to return hits from
  the whole shared arena and each package discarded the foreign and
  masked-out nodes in JS afterwards; a Rust mover needs the filter inside
  its loop, so `flux:spatial`'s raycast/overlap/sweep/moveAndSlide take a
  `{ root, layers, nodes }` filter, nodes carry a layer mask
  (`setLayers`), and both packages scope queries to their root node (the
  2d layer gained one; parentless sprites and groups hang off it). The
  3d `meshes` include-list is the node-id list, instances expanded.
- The mover is Rust (`alloy/src/spatial/mover.rs`), one FFI call per body
  per frame instead of the JS loop's ten-odd overlaps and sweeps. The 3d
  package's `moveAndSlide` is now `scene.moveAndSlide` plus the free
  spelling, a pack-and-map wrapper; the 2d layer's is the same core call
  with `up` lifted into the plane. The controller cases from
  `packages/3d/tests/collision.test.ts` (analytic planes) moved to
  `alloy/src/tests/spatial_collide.rs` over real quads.
- Sprites are columns in the index, not flat quads. The shaping's "unit z
  extent so every sprite plane lies inside it" packs the volume, but the
  core's contact for a capsule whose segment lies in a flat triangle's
  plane is the plane normal with depth = radius, so a circle over a
  sprite would have pushed out along z. With a large z half-extent on the
  sprite's index box every volume at z = 0 meets side faces only, and
  contacts, sweeps and the mover stay in the plane with no 2d narrowphase.

The mover is a layer method (`layer.moveAndSlide`), not a free function,
matching `pick`. `QueryOptions` is the `sprites` include-list only:
sprites carry no layer bits yet, and the core mask is in place for when
they do. `packages/2d/checks/collision-check.tsx` is the probe from
"done looks like": a capsule lands on a floor of tiles a skin short,
stops at a wall with `wall` while keeping the floor, and climbs a slope
(a rotated sprite) reporting it as floor; a blast circle finds three
tiles with in-plane contacts; a shot's sweep and raycast agree on the
first tile; `pickRect` equals `overlap` over the rect.
