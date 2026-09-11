---
title: Polyhedron primitives - the platonic solids and the icosphere as geometry builders
description: Three's PolyhedronGeometry family (tetrahedron, octahedron, icosahedron, dodecahedron and the generic builder with detail subdivision) ported into @solidrt/3d as five pure generators next to sphere(), giving the flat-shaded solids at detail 0 and the icosphere above it; Godot and Unity ship neither, and the reason why shaped what the item does not claim.
created: 2026-09-11
completed: 2026-09-11
---

# Polyhedron primitives - the platonic solids and the icosphere

## Symptom

`sphere()` was the only round primitive, a lat/long grid whose triangles
collapse into slivers at the poles. Three's low-poly look, the one most
of its demos and many ports carry, is `IcosahedronGeometry` with flat
shading, and its smooth sphere for displacement and procedural shading is
the same geometry with `detail` above 0. Neither had a counterpart, and
[three-feature-survey](../notes/three-feature-survey.md) listed the family
as untracked.

## Where this sits

| | Three | Godot | Unity |
| --- | --- | --- | --- |
| Polyhedra | `Tetrahedron/Octahedron/Icosahedron/DodecahedronGeometry(radius, detail)` over `PolyhedronGeometry(vertices, indices, radius, detail)` | none | none |
| Icosphere | `IcosahedronGeometry` with `detail` | none; `SphereMesh` is lat/long, CSG spheres too; proposals open | none; ProBuilder's Icosphere in the modeling package |
| Default radius | 1 | n/a | n/a |
| Output | non-indexed, flat normals at detail 0, radial above | | |

Godot and Unity are not withholding a feature; their primitives are
blockout shapes and modeling is expected to happen in Blender, so a mesh
you cannot greybox with belongs in an imported asset or an `ArrayMesh`.
Unity's set also mirrors its collider shapes, to which an icosphere adds
nothing. Three has no import pipeline behind it, so code is the modeling
tool and its geometry list is a kit. SolidRT is on Three's side by
construction: no editor, and `torusKnot`, `lathe`, `extrude` and
`sweep` already make the generator set a kit. The polyhedra extend it.

The two things taken from the other side: the textured sphere stays
`sphere()`, because the polyhedra's spherical projection stretches toward
the poles and Three's seam patch is a per-triangle heuristic (its own
comment calls the threshold arbitrary); and nothing touches the collision
volumes, which are analytic already and do not care how a visual sphere
is triangulated.

## Shape

Five generators in `src/geometry.ts` after `sphere()`, exported from
`index.ts` with one `PolyhedronOptions` type:

- `polyhedron(vertices, indices, { radius, detail })`, the generic
  builder over Three's data form: a flat xyz list and a CCW triangle
  list. The data goes first like `box3Helper(bounds, options)` so the
  options object keeps the generators' rule that every field is
  optional; the first draft put the data in the options object.
- `tetrahedron`, `octahedron`, `icosahedron`, `dodecahedron`, each
  `({ radius, detail })` over Three's corner tables, which port verbatim
  since their winding is already CCW from outside.

Three decisions:

1. **Non-indexed output, Three's form.** Every triangle owns its three
   vertices and the indices run 0..n-1. That is what lets the normals
   switch on `detail`: face normals at 0 so a dodecahedron reads as
   twelve flat pentagons, radial normals above so the icosphere is
   smooth. A welded, indexed form is additive: the survey's
   `mergeVertices` op would weld any geometry, this one included, and
   `edgesGeometry`/`wireframeGeometry` already weld by position. Grid
   points are integer-weighted means of the face corners summed in
   corner order, so a point on a shared edge is the same two-term sum
   from both faces and lands bit-identical, one vertex to the weld
   rather than two a rounding step apart.
2. **Default radius 0.5, not Three's 1.** `sphere()`, `cone()`,
   `torus()` and `circle()` all default to 0.5 (unit diameter, matching
   the unit `box()`), so `icosahedron({ detail: 3 })` drops in for
   `sphere()` at the same size. The doc line notes the divergence, as
   `torusKnot` does for orientation.
3. **UVs in Three's spherical map, in the package's v convention.** v
   runs 0 at the top like `sphere()` and `box()`, which is Three's map
   without its `1 - v` flip; the u seam and y-axis corrections port as
   they are. Three's own icosphere seam sits 180 degrees from its UV
   sphere's, and that is kept rather than realigned so a ported texture
   lands where it did in Three.

`detail` that is not a non-negative integer throws, per the dev
validation policy; a ragged vertex or index list throws too.

## Done looks like

`checks/geometry-check.ts` covers each solid at detail 0, 1 and 3:
triangle counts (F times (detail + 1) squared), every corner on the
circumsphere, CCW winding seen from outside, face normals at 0 and radial
above, UVs in range with no triangle left straddling the seam and y-axis
corners taking their triangle's azimuth, the edge counts through the
position weld (the dodecahedron's fan diagonals are coplanar, so
`edgesGeometry` gives its twelve pentagons while `wireframeGeometry`
lists the 54 triangulation edges), the generic builder over an open
face, and the one-pass colored layout. `examples/polyhedra.tsx` puts the
flat solids, the detail 1 to 3 progression and a wireframe pair (a 12 by
8 UV sphere beside a detail 2 icosphere, about 170 triangles each) on
screen, so the pole slivers against the uniform triangles are the
snapshot.

## Not in this item

- **A welded form.** Non-indexed is Three's contract and the one the
  normals switch needs; a general `mergeVertices` weld is the survey's
  geometry-operation item and would serve this family too.
- **A flat/smooth toggle independent of `detail`.** Three ties them and
  no port needs the split; a `flat` option lands additively if one does.
- **Realigning the icosphere seam with `sphere()`'s.** Parity over
  tidiness, see decision 3.
