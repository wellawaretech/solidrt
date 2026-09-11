---
title: Normals cannot be computed or recomputed
description: Nothing in the geometry surface generates normals, so a hand-authored vertex array, a geometry deformed through updateVertices and anything wanting flat shading all have no path; Three, Godot and Unity all ship it and all mutate in place, where our packed Geometry wants the withColors/transformGeometry copy shape instead.
created: 2026-09-11
completed: 2026-09-11
---

# Normals cannot be computed or recomputed

There is no `computeVertexNormals` anywhere. Normals only ever arrive from
a generator or a loader, so three cases have no path at all:

- A hand-authored vertex array packed with `packGeometry`: the normal slot
  has to be filled by hand or the surface shades wrong.
- Geometry deformed after the fact (`updateVertices`): positions move,
  normals stay, lighting goes stale.
- Flat shading, which needs per-face normals on split vertices - blocked
  twice over, since `toNonIndexed` is missing as well (same survey entry).

The most glaring gap in [three-feature-survey](../notes/three-feature-survey.md)
now that the debug helpers have landed.

Three/Godot/Unity all ship it and all three MUTATE in place:
`BufferGeometry.computeVertexNormals()` (area-weighted, no angle),
`SurfaceTool.generate_normals(flip)`, `Mesh.RecalculateNormals()`. A
3-vs-0 convergence, and the one we should still diverge from: our
`Geometry` is a packed buffer rather than an object holding attribute
arrays, and the established op shape is a copy - `transformGeometry`,
`mergeGeometries`, `withColors`, `withAttribute`, `withMorphTargets` all
take a `Geometry` and return a new one. So the leading candidate is
`withNormals(geometry, options?)`, which reads with that family. There is
also an in-place family it could join instead (`fillAttribute`,
`fillColors` write into the buffer and return the view), and that is the
first thing to settle.

Open shape questions beyond that: smoothing behaviour (Three averages
every shared vertex with no angle knob and puts the crease control in a
separate `toCreasedNormals`; our profile builders already carry a `smooth`
flag per point, so the vocabulary exists), whether an indexed geometry
averages across the index or is split first, and which vertex layouts
qualify - the normal slot is a named attribute under the open layouts, so
this rides the accessors from
[3d-vertex-data-model](../done/3d-vertex-data-model.md) rather than
assuming the standard layout.

Adjacent and cheap once this exists: `center`, `normalizeNormals` and
`computeBoundingSphere`, all small over `geometryBounds` and
`transformGeometry`.

## Decision

The copy-versus-in-place question settled on the rule the package already
had: an op that keeps the vertex count writes in place (the `fill*`
family), one that changes the count or the layout returns a copy (the
`with*` family and the derived builders). Normals split along exactly
that line, so both shapes shipped, each pure:

- `computeVertexNormals(geometry)` in place, by index, no options - the
  3-of-3 convergence (Three, Unity, Godot all mutate) and the only shape
  the per-frame deformation loop can use, since a copy is new GPU
  buffers and a `setGeometry`. Returns the stream carrying aNormal like
  `fillAttribute`, for `updateVertices`.
- `withNormals(geometry, creaseAngle = 60, label?)` the authoring copy:
  `toNonIndexed`, per-corner normals over faces matched by position
  within the angle, `mergeVertices` by every channel. The weld is what
  makes the split minimal, so the result is indexed and split only where
  a crease or a uv seam needs it; it also absorbs the missing-channel
  case (a copy owns its layout). A first draft put the crease angle on
  the in-place op and "degraded" at shared vertices; that was the
  least-effort shape and was dropped for this one.
- `toNonIndexed` and `mergeVertices` exported as the index pair; the
  `normalsHelper` builder rode along as the verification tool, on the
  static-builder split the debug helpers settled.

Both computations weight a face by its corner angle rather than by area
(Three) or not at all (Godot): area weighting counts a quad's diagonal
faces twice at two of its corners, so a fully smoothed cube leaned off
its corner diagonals in the check rig. Corner angles make the result
independent of triangulation, and the cube's diagonal comes out exact.

Left out on purpose: `center` (one `transformGeometry` over
`geometryBounds`, the recipe is in AGENTS.md), `normalizeNormals`
(computed and transformed normals are unit already) and
`computeBoundingSphere` (Unity and Godot stop at the box, and so does
every query and the LOD here). The remaining hand-authoring friction,
`packGeometry` wanting 8-float interleaved rows where a porter holds
planar position, uv and index arrays, is a `packGeometry` shape
question and not filed here.

## The in-place op moved to the core the same day

The first cut ran `computeVertexNormals` in JS through the accessor
closures: about 11 ms per frame for a default sphere plus its helper
refill in QuickJS, an interpreter loop over every vertex every frame,
which is the class of work this engine keeps out of JS everywhere else.
It now lives in `alloy::spatial::vertex_normals` (corner-angle weighted,
pinned by `alloy/src/tests/spatial_normals.rs`) behind
`flux:spatial.computeNormals`, one call over the vertex floats, with the
JS side in `geometry-gpu.ts` resolving the layout only. The authoring
ops stay JS: they run once at build time. The docs now say where
per-frame deformation belongs: a vertex shader, with the CPU loop kept
for data that genuinely changes on the CPU.
