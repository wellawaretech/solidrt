---
title: Capsule primitive and helper - the collision volume gets a mesh and a gizmo
description: capsule() as a geometry generator next to sphere() and cylinder(), height the total extent as in Godot and Unity, and capsuleHelper(volume) drawing the { a, b, radius } collision volume as lines; closes the survey's asymmetry of a Capsule query volume nothing could draw.
created: 2026-09-11
completed: 2026-09-11
---

# Capsule primitive and helper

## Symptom

`Capsule` was a collision volume for `overlap`/`sweep`/`moveAndSlide`
and nothing could draw one: no generator for the character-controller
shape every engine ships, and no outline of the volume to see what a
query actually tested. [3d-feature-parity](../notes/3d-feature-parity.md)
listed `CapsuleGeometry` as the missing primitive and roadmap item 10
carried it as a tube special case.

## Where this sits

| | Three `CapsuleGeometry` | Godot `CapsuleMesh` | Unity capsule | our `Capsule` volume |
| --- | --- | --- | --- | --- |
| radius | 1 | 0.5 | 0.5 | `radius` |
| height | 1, the middle section only | 2, total, clamped to 2r | 2, total | segment `a`..`b` |
| cap tessellation | `capSegments` 4 per cap | `rings` 8 per cap | fixed | n/a |
| around | `radialSegments` 8 | `radial_segments` 64 | fixed | n/a |
| band | `heightSegments` 1 (r171) | 1 | fixed | n/a |
| axis | y | y | y | free |
| volume gizmo | none | collision shape gizmo | collider gizmo | none |

## Shape

`capsule({ radius = 0.5, height = 1, capSegments = 8, radialSegments =
24, heightSegments = 1 })` in `src/geometry.ts` after `cone()`, and
`capsuleHelper(volume, { segments = 32 })` with the debug helpers after
`box3Helper`. `cylinder()` (and so `cone()`) gained the same
`heightSegments`, Three's option it had been missing, so the two
y-axis bodies subdivide alike.

1. **`height` is the total extent**, Godot and Unity against Three. It
   is also what `cylinder({ height })` means, so the default capsule
   fills the same unit box as the default cylinder and sphere. Three's
   `height` is ours minus 2r; the doc line says so. `height: 2 *
   radius` is a sphere and less throws (Godot clamps silently; the dev
   validation policy says throw).
2. **The sphere grid split by the band.** `capSegments + 1` rows per
   hemisphere with each cap's center shifted to its end of the segment,
   the band `heightSegments` cell rows between the two equator rows
   (deformation, gradients and skinning need rows to move), indices from
   `gridIndices` with both poles collapsed exactly as `sphere()`.
   Normals radial from each cap's center, so the band shades as a
   cylinder; v by arc length pole to pole. A `lathe()` of a sampled
   half circle would have been fewer lines but emits a degenerate
   zero-radius strip along the axis, averages the pole normals with it,
   and maps v differently from `sphere()`.
3. **The helper takes the volume form**, `{ a, b, radius }` in any
   orientation, exactly what a query took, and draws it in the space it
   was given (a scene-space volume goes under an identity node, like a
   `box3Helper` of query bounds). Rings at both segment ends, four
   lines between them, two half circles per cap in perpendicular planes;
   `a == b` draws a sphere's three great circles with the duplicate ring
   and the zero-length lines dropped. The mesh primitive stays axis
   aligned and parameterized by radius and height because all three
   engines do it that way and leave orientation to the node transform.
   The conversion between the two forms is one line in the doc.

## Done looks like

`checks/geometry-check.ts`: vertex and index counts, bounds from the
total height, every normal unit and radial from its own cap's center,
every vertex on its cap sphere, v monotone from 0 at the top pole to 1
at the bottom, the `height = 2r` sphere bounds, the too-short throw and
the one-pass colored layout; for the helper, every vertex at `radius`
from the segment with its normal pointing away from the nearest segment
point, the counts, the sphere case, and the segments validation.

## Not in this item

- **Folding `sphere()` into `capsule()`.** The zero-segment case is a
  sphere with a duplicated equator row; the textured default stays the
  20-line `sphere()`.
