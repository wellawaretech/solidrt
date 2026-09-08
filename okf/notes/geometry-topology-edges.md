---
title: Geometry topology and edge builders - what the implementation taught
description: Why topology belongs on the geometry and not the material, why edge builders must weld by position, the facet-angle rule for edgesGeometry thresholds, and the glTF primitive-mode mapping.
created: 2026-09-08
---

# Geometry topology and edge builders

From [3d-geometry-topology](../done/3d-geometry-topology.md).

- The picking shape, the pipeline and the validation all keyed off one
  fact the geometry did not carry. Once `Geometry.topology` exists, all
  three follow it and the material-side `topology` has no case left in
  the 3d package: every draw has a geometry (sprites their quad,
  instanced meshes theirs). It survives only in `@solidrt/core/gpu`,
  where a raw pipeline genuinely has none.
- `pipelineFor` was already lazy per layout; adding topology to its key
  was the whole pipeline change. Override and shadow materials go
  through the same call, so they draw lines as lines without a case.
- `spatial.setShape(node, null)` was already a supported state (sprites
  use it) and the core box-falls-back for raycast, overlap and sweep, so
  a lines geometry is box-picked rather than unpickable, with no core
  change.
- Edge builders must weld by position, not by index: a box() has 24
  split vertices, so an index-keyed wireframe lists every cube edge
  twice (one per face) and an index-keyed edges pass marks all of them
  as borders. Three's EdgesGeometry welds for the same reason;
  WireframeGeometry does not and shows the double lines.
- edgesGeometry's 1-degree default (Three's) keeps every facet line of a
  generated round shape: the facet angle is 360 / radialSegments, 15
  degrees for the default cylinder and sphere. The threshold must pass
  it to hide the seams, and a threshold exactly at the facet angle sits
  on a floating-point boundary (some seams show, some hide), so the
  docs and the check use 16.
- The glTF loader silently returned for any `mode` other than 4, so
  line and point primitives vanished from a model. Strips and fans are
  unrolled to triangle lists at load (what the mirror flip, the
  flat-shading un-index and the shape expect); a line loop has no GPU
  topology and closes into a strip.
