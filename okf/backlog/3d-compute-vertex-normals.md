---
title: Normals cannot be computed or recomputed
description: Nothing in the geometry surface generates normals, so a hand-authored vertex array, a geometry deformed through updateVertices and anything wanting flat shading all have no path; Three, Godot and Unity all ship it and all mutate in place, where our packed Geometry wants the withColors/transformGeometry copy shape instead.
created: 2026-09-11
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
