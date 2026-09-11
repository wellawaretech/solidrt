---
title: Four names in the 3d public surface say the wrong thing
description: shape() wears Three's name for the INPUT class on a geometry output, MAX_SHADOWS is the light cap rather than the shadow budget it names, the lit/standard pair mixes two engine vocabularies with URP's meaning of Lit inverted, and "standard" names both a vertex layout and the PBR material on axes that never correlate; each is a rename with no compat constraint.
created: 2026-09-11
---

# Four names in the 3d public surface say the wrong thing

Four independent findings, each small, grouped because they are one pass
over the same surface. Ordered by how likely a porter is to be misled.

## 1. shape() wears Three's input name on a geometry output

`shape(profile, options?)` returns a flat filled `Geometry`. Three ways it
sits wrong against its own neighbourhood:

1. **It names the category, not the shape.** Every sibling says which shape
   comes out - `box`, `sphere`, `circle`, `ring`, `cone`, `cylinder`,
   `torus`, `torusKnot`, `capsule`, `tetrahedron`, `polyhedron`. `shape`
   says "a shape", and its doc has to define it relationally instead: "the
   general case of `circle()`/`ring()` for arbitrary outlines". The general
   case of a circle for an arbitrary outline is a polygon.
2. **It reuses Three's INPUT name for the output.** In Three `Shape` is the
   2D path class you pass in (the one carrying `.holes`) and
   `ShapeGeometry` is what comes out; ours inverts both halves, since our
   input type is `Profile`. The package otherwise relates to Three's names
   two consistent ways - verbatim (`edgesGeometry`, `wireframeGeometry`) or
   minus the suffix (`box`, `torusKnot`, `polyhedron`). This is the only
   name that does neither.
3. **It breaks its own file's rule.** `profile.ts` states the split in its
   header: it holds "the 2D outline vocabulary the solid generators build
   on", and "the swept-solid generators consuming this vocabulary -
   extrude, lathe, sweep, tube - live in sweep.ts". `shape` is a generator
   consuming that vocabulary and it is the lone exception; its neighbours
   return profile points (`fillet`, `roundRect`) or indices
   (`triangulate`).

Three/Godot/Unity: no convergence to respect. Three is the only one with a
convention here (`Shape` in, `ShapeGeometry` out). Godot has no flat-fill
3D primitive (`Polygon2D` is 2D, `CSGPolygon3D` covers extrude/lathe/
sweep); Unity has no procedural primitive API beyond `CreatePrimitive`.

Candidate: `polygon(profile, options?)` in `sweep.ts`. It names the
specific shape like every sibling and pairs with `polyhedron(vertices,
indices, options)` as the 2D/3D generic-by-data couple. `fill` is not a
candidate: `fillAttribute`, `fillColors` and `fillet` already hold that
prefix.

The forward-looking argument is the strongest one. `Shape.holes` is an open
gap ([three-feature-survey](../notes/three-feature-survey.md), Curves and
shapes): an outline plus holes is exactly what Three calls `Shape`, so
leaving the name on the output squats the one we would want for the input.

## 2. MAX_SHADOWS is the light cap, not the shadow budget

`light.ts` exports `MAX_SHADOWS = MAX_LIGHTS`, and its own doc comment
immediately corrects the name it just gave: "Every light may cast; the real
bound is the shadow-slot budget (MAX_SHADOW_MAPS: a directional light
claims `shadow.cascades` slots, a point light six, a spot one...)". So the
only shadow-named constant a consumer can import is the LIGHT cap, and the
actual budget its doc points at is not exported at all.

Both are 8 today, which is why nothing has broken: an app sizing its
casters against `MAX_SHADOWS` gets the right answer by coincidence. It
becomes wrong the moment either constant moves, and `MAX_SHADOW_MAPS`
carries a comment explaining that it is deliberately its own constant "NOT
MAX_LIGHTS * MAX_CASCADES, so raising the light cap does not size the
fragment uniform budget" - the decoupling exists precisely so they can
diverge.

Done looks like: export the real budget under the name that describes it,
and either export the light cap as `MAX_LIGHTS` or drop the alias. One
cascaded sun already claims four of the eight slots, so the distinction is
load-bearing for anyone counting.

## 3. lit/standard mixes two vocabularies, with Lit inverted

`lit()` is Lambert diffuse plus an optional Blinn-Phong highlight;
`standard()` is metalness/roughness GGX. The trio `unlit`/`lit`/`standard`
draws `unlit` and `lit` from Unity URP and `standard` from Three, Godot and
Unity's built-in pipeline - and URP's meanings are the reverse of ours:

| | ours | Unity URP |
| --- | --- | --- |
| Blinn-Phong | `lit` | `Simple Lit` |
| metal/rough PBR | `standard` | `Lit` |

A URP porter reaching for `lit` gets the material URP calls Simple Lit.
Three has no "lit" at all (`MeshPhongMaterial` / `MeshStandardMaterial`),
and Godot has one `StandardMaterial3D` with a `shading_mode`. The
asymmetry shows in our own docs: `standard()` cites all three engines by
name, `lit()` cites none.

This is the silent-mismatch class `packages/3d/CLAUDE.md` exists to catch,
so it wants the full three-way pass rather than a guessed rename. The
options worth weighing are naming `lit` for its model (it is Blinn-Phong,
and saying so removes the ambiguity outright) against renaming the pair
onto one vocabulary end to end.

## 4. "standard" names two independent things

`VertexLayout` is `"standard" | "colored" | "skinned"`, with
`VERTEX_LAYOUTS.standard` and `STANDARD_FLOATS = 8`. `standard()` is the
PBR material. The two axes never correlate: `standard()` runs against
`"skinned"` and `"colored"` geometry, and `lit()`/`unlit()` run against the
`"standard"` layout. So the shared word predicts nothing, in a package
where layout/material mismatches are already a real error class.

The layout is the side to rename: the material name is pinned by Three,
Godot and Unity converging on it, while the layout vocabulary is ours
alone.

## Shape gate

Each rename carries the standing gate from `packages/3d/CLAUDE.md`: the
Three/Godot/Unity comparison in the proposal. There is no compat
constraint, so all four are mechanical once named.
