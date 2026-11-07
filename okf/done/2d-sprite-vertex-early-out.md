---
title: The 2d sprite vertex stage does its full work for sprites that draw nothing
description: The stock sprite vertex stage unpacks a hull layer's whole mesh before its zero-size check and places a zero-size sprite through the camera in full, and every one of a hull sprite's 8 vertices unpacks all 8 mesh points; on a record layer that keeps empty slots this turns hulls into a net loss (46.6 ms a drag frame against 42.9 with quads, 30,703 records on an Adreno 610) - wanted a zero-size early-out and three points per vertex, the cost view culling leaves.
created: 2026-11-06
completed: 2026-11-06
---

# The 2d sprite vertex stage does its full work for sprites that draw nothing

## Symptom

Every record of a sprite or record layer runs the stock vertex stage
(`unlitVertex`, `spriteCornerSource` and `SPRITE_VERTEX_BODY` in
`packages/2d/src/glsl.ts`) on all its vertices, and two parts of it do
work whose result is thrown away:

- **Zero-size sprites.** On a hull layer `spriteCorner` unpacks the 8
  mesh points from the record's six mesh floats first, and only then
  tests `extent.x == 0.0 || extent.y == 0.0` and returns the quad
  corner. On either layer kind the collapsed corner then goes through
  the whole `SPRITE_VERTEX_BODY` (rotation, camera, varyings) to land on
  the sprite's centre. A zero-size record is the normal way to keep a
  slot free: a layer reserved with `capacity`, a slot kept for an icon a
  piece does not show yet, a particle pool at rest.
- **Every vertex repeats the whole hull.** Each of a sprite's 8 vertices
  unpacks all 8 points and scans the ring both ways for its two distinct
  neighbours, so a sprite unpacks 64 points to place 8. This is paid by
  every drawn hull sprite, not only by the empty ones.

Measured on an Adreno 610 tablet (display scale 1.5, view oversample
1.5, 0.0.86), a record layer of 30,703 records, about 650 sprites on
screen and two thirds of the records zero-size, a scripted 3 s drag:

| drawn as | vertices | GPU per frame | view pass | outside the pass |
|---|---|---|---|---|
| hulls | 245,624 | 46.6 ms | 27.1 ms | 19.5 ms |
| quads | 122,812 | 42.9 ms | 31.9 ms | 11.0 ms |

The part outside the pass is the vertex work (it does not move when only
the fragment stage changes). The hull saves 4.8 ms of fragments and
costs 8.5 ms of vertices here, a net loss of 3.7 ms; on a layer of 613
sprites filling the same screen the same hulls win (a hull view pass of
29.8 ms against a quad view pass of 35.7 ms). So the hull's vertex cost
suits a few hundred large sprites and not tens of thousands of records
that are mostly empty or off screen.

## Decided

- **Off-screen sprites are view culling's.** Most of this table's vertex
  work is records off screen, and
  [2d-sprite-view-culling](2d-sprite-view-culling.md) stops submitting
  them, as engines do (a renderer's bounds or a chunk tested before the
  draw, not in the vertex stage). A bounding-circle test in the vertex
  stage would only add to that the sprites just off screen inside a
  visible cell, which is close to nothing, and layers that leave cells
  off are what culling's option exists for. So this item has no
  viewport test.
- **What stays is what culling does not touch.** The zero-size slots a
  layer submits (inside visible cells, and on every layer without cells:
  a particle pool at rest or a reserved layer sits in one place, where a
  cell grid does not help), and the per-vertex cost of every drawn hull
  sprite. Engines collapse a free slot the same way: Unity's VFX Graph
  output returns a degenerate vertex for a dead particle; Godot zeroes a
  free MultiMesh slot's or an inactive particle's transform, without
  the early-out, so its vertex stage still runs in full.

## Done looks like

- **Size first.** On sprite and record layers, quad and hull atlases
  alike, the stock stage tests the drawn size before `spriteCorner` and,
  for a zero-size sprite, writes one fixed position for every vertex (a
  degenerate primitive, no fragments) and returns, before any mesh
  unpack or camera work. The size is safe to test after the screen-size
  clamp: `screenSizeScale` returns 1 for a collapsed quad, so a zero
  size stays exactly zero.
- **A helper for custom stages.** A material's own vertex stage calls
  `spriteCorner` itself, so the test is exported as a GLSL function in
  the prepended corner source (say `spriteDrawsNothing(vec2 size)`), and
  `packages/2d/agents/materials.md` says to call it first, where it
  already describes `spriteCorner`. It takes only the size, so it needs
  none of the camera uniforms the stage declares after the corner
  source. A named function keeps the contract one call if the test ever
  grows.
- **Three points a vertex, not eight.** A vertex needs its own point and
  its two distinct neighbours. Reordering the packed points cannot give
  that: the padding repeats the last ring point (`packMesh` in
  `sprite-mesh.ts`), so on a 5-gon the strip slots 5 to 7 must land on
  point 4's pushed position and need point 4's neighbours (3 and 0), not
  their own k - 1 and k + 1. Instead the pack step stores the ring's
  point count m in one of the field's two spare bytes (6 floats of 3
  bytes hold 18, the 8 points use 16), and `spriteCorner` takes
  `k' = min(k, m - 1)` and unpacks points k' and `(k' +- 1) mod m` only.
  The read selects its 3 points with plain bit selects on the six
  words: a local array indexed at run time may spill to scratch memory
  on Adreno, which would eat the saving. The drawn strip and its pixels
  stay the same.
- **Measured.** The table above again on the same device, both rows
  (the size test speeds up the quad layer too, so the quads' row of
  today is not the bar), and the 613-sprite layer's hull win, which the
  three-point read should widen. Lanes diverge in a wave that mixes live
  sprites with dead ones and the wave pays the full path, so measure
  the size test and the three-point read apart. The existing tests in
  `packages/2d/tests/hull-meshes.test.tsx` keep the hull layer's image
  equal to the quad layer's, which covers the three-point read; one more
  shows a hull sprite whose mesh has fewer than 8 points (the padded
  ring) draws the same image as its quad. A zero-size sprite already
  draws no pixels today, so only the device measurement shows the size
  test's saving.

## Involves

`packages/2d/src/glsl.ts` (the test in `unlitVertex`, the helper in
`spriteCornerSource`, the three-point unpack), `packages/2d/src/sprite-mesh.ts`
(the point count in the packed field), `packages/2d/agents/materials.md`,
`packages/2d/tests/hull-meshes.test.tsx`.

Related: [2d-sprite-view-culling](2d-sprite-view-culling.md) removes the
part of this table's cost that follows the world size; this item makes
what is still submitted cheaper. Both items quote the same 19.5 ms, so
whichever lands second re-measures its expected win.

## Outcome

Built as shaped. The point count is byte `MESH_COUNT_BYTE` (16) of the
mesh field, and `unpackPoints` reads that many points. The corner source
on both layer kinds declares `spriteDrawsNothing(size)` and the position
`SPRITE_NOWHERE` (past the far plane). The stock stage writes that
position for a zero-size sprite and returns before `spriteCorner`.
`spriteCorner` still collapses a zero-size sprite for a stage that skips
the helper, and it does this before it reads the mesh. The neighbour
scan and its "no distinct neighbour" guard are gone: the ring holds at
least three distinct points by construction, and `sprite-mesh.test.ts`
checks this and the padding on every random mesh.

Measured on the same tablet (Galaxy Tab A7, schedutil on all 8 cores,
thermal status 0, battery 24 C, USB power), with the Meinstein boards
of `../demo/tiles` on 0.0.86. Only `glsl.ts` and `sprite-mesh.ts` were
swapped. Each row is three runs of the scripted drag, each after a
fresh reload of the same deal, and the runs agreed within 0.2 ms:

| 30,703 records (nuts board) | GPU per frame | view pass | outside the pass |
|---|---|---|---|
| hulls, 0.0.86 | 46.6 ms | 27.1 ms | 19.5 ms |
| hulls, size test only | 43.5 ms | 27.1 ms | 16.4 ms |
| hulls, size test and three points | 41.6 ms | 27.0 ms | 14.6 ms |
| quads, 0.0.86 | 43.0 ms | 32.0 ms | 11.0 ms |
| quads, size test | 43.0 ms | 32.1 ms | 10.9 ms |

| 613 records (large board) | GPU per frame | view pass | outside the pass |
|---|---|---|---|
| hulls, 0.0.86 | 36.5 ms | 27.3 ms | 9.2 ms |
| hulls, both | 36.0 ms | 27.2 ms | 8.8 ms |
| quads, 0.0.86 | 40.8 ms | 32.3 ms | 8.6 ms |
| quads, size test | 40.8 ms | 32.3 ms | 8.6 ms |

- The size test saves 3.1 ms on the hull layer and nothing on the quad
  layer. A quad vertex does almost no arithmetic, so what is left there
  is the fixed cost of each vertex (the instance fetch), which an
  early-out cannot remove. The quad row of today is the bar after all.
- The three-point read saves another 1.9 ms on the large layer, and
  0.4 ms on the 613-sprite one.
- Hulls now win on both layers: by 1.4 ms on the 30,703-record board
  (they lost by 3.6 ms), and by 4.8 ms on the 613-sprite board (4.3
  before).
- On the 613-sprite board "outside the pass" is 8.6 ms for 10,156
  vertices. That part of the frame is not all vertex work: about 8.5 ms
  of it is there whatever the layer submits.

