---
title: Geometry carries its topology, so the picking shape stops assuming triangles
description: Every geometry attached to a 3d scene gets a triangle picking shape built from its index buffer, so a lines or points geometry throws "shape indices must be a triangle list" from a call the app never made, and a strip geometry that happens to divide by three gets a shape of garbage triangles; topology belongs on the geometry (as it does in Three, Godot and Unity), and the shape should follow it.
created: 2026-09-08
completed: 2026-09-08
---

# Geometry carries its topology

## Symptom

`acquireGeometryBuffers` (`packages/3d/src/geometry-gpu.ts`) builds the
spatial core's picking shape for every geometry it uploads:

```ts
shape: createShape(geometry.vertices, layoutStride(geometry.layout), 0, 6, geometry.indices),
```

and `Shapes::create` (`alloy/src/spatial/pick.rs`) rejects an index count
that is not a multiple of three. So a geometry whose indices are not a
triangle list cannot be attached to a scene at all, whatever its material
draws it as. `ShaderMaterialClassOptions.topology` accepts `"points" |
"lines" | "line-strip" | "triangles" | "triangle-strip"`, so the API
offers four topologies that no geometry can legally feed.

Two things are wrong at once:

- **The throw is unreachable from what the app asked for.** It arrives at
  `add()` or `setGeometry`, from a call the app never made, in picking
  vocabulary, about an index buffer the app deliberately built as edge
  pairs.
- **The count is not the question.** An edge list is two indices per edge
  and lands on a multiple of three about one time in three, so the same
  code either throws or silently builds a shape whose "triangles" are
  three unrelated edge endpoints. Strips are worse: an indexed
  `triangle-strip` that divides by three passes the check and produces
  triangles that are not the ones drawn. Picking then returns wrong hits
  instead of an error, which is the worse failure.

The concrete case that hits this is a wireframe pass: the same vertices
and the same layout with an EDGE index buffer, swapped onto a live mesh
with `setGeometry`/`setMaterial` against a `lines` shader material. That
half works exactly as designed and is a dozen lines; only the shape check
needs a workaround (padding the edge list with degenerate lines until the
count divides by three, which leaves a meaningless shape in the index).

## Why topology belongs on the geometry

Today `topology` is material state, and the picking shape is cached per
`Geometry` in a WeakMap and shared by every mesh and scene drawing it. So
"build the shape from the material's topology at attach" cannot work:
two materials of different topology may share one geometry, and there is
one shape for both.

The prior art agrees. Three has `LineSegments`/`Points` as object types
over a plain `BufferGeometry`; Godot puts the primitive type on the mesh
surface; Unity takes it in `Mesh.SetIndices(..., MeshTopology.Lines)`.
In all three the index buffer and the primitive it describes travel
together, because they are one fact: an index buffer means nothing
without the topology that reads it.

## What shipped

- `Geometry.topology?: Topology`, absent = `"triangles"`, carried through
  `withAttribute`, `transformGeometry` and `mergeGeometries` (which
  rejects mixed topologies and strips). `geometryTopology` applies the
  default.
- `validateGeometry` owns the count rule per topology in geometry
  vocabulary at `add()`. A `lines` or `points` geometry may be empty and
  draws nothing: a feature-edge pass over a smooth closed part has no
  edges, and that is a result, not a bug.
- `Material.pipeline(layout, topology)`: one pipeline per pair, keyed
  like the layout already was. Override and shadow materials go through
  the same call. `ShaderMaterialClassOptions.topology` is gone; topology
  lives on the geometry only.
- The picking shape is built for triangle lists only; other topologies
  attach with `shape: null` and pick and collide by their box, the
  sprite/instanced tier the core already had.
- Vertex uploads are keyed on the `Float32Array`: geometries sharing a
  vertex array share one GPU buffer.
- `wireframeGeometry(geometry, label?)` and `edgesGeometry(geometry,
  thresholdAngle?, label?)`: `lines` geometry over the source's own
  vertices and layout, edges welded by position.
- The glTF loader maps every primitive mode: lines, line strips and
  points keep their topology, strips and fans unroll to triangle lists,
  a line loop closes into a strip, an unknown mode throws naming the
  primitive.
- Lines and points are skipped by shadow views.
- `examples/wireframe.tsx`, the geometry and glTF check rigs, AGENTS.md.

Verified on the running example: all nine draws switch from
`triangles` to `lines` on one new unlit pipeline with every vertex
buffer id unchanged (shared upload), the empty dome edges attach and
draw nothing, snapshots of the three modes show the expected lines.

## Non-goals

- A segment narrowphase for picking lines (Three's `Line.raycast` with
  a threshold) is a spatial-core feature; box picking is the tier until
  it exists.
- Wide lines: GL ES draws one-pixel lines; thick lines are quad geometry
  (Three's `Line2`).

Findings are in [geometry-topology-edges](../notes/geometry-topology-edges.md).
