---
title: Debug helper builders - grid, axes, bounds box and plane as lines geometry
description: Nothing in @solidrt/3d draws a grid, an axis triad, a bounds box or a ground plane, which is the first cluster a porter reaches for because it is what you use while debugging the port itself. With topology "lines" on Geometry these are pure builders next to wireframeGeometry, so the item is four functions plus the one missing material flag - vertexColors on unlit - and no components, renderer or Rust work.
created: 2026-09-10
completed: 2026-09-10
---

# Debug helper builders - grid, axes, bounds box and plane as lines geometry

## Symptom

Nothing in `@solidrt/3d` draws a grid, an axis triad, an object's bounds
or a ground plane. Three ships all of them as `LineSegments` subclasses
you `scene.add()`, and they are the first thing a porter reaches for,
because they are what you use while debugging the port itself: is my
model at the origin, is it Y-up, how big is it, where is the ground. Today
the answer is to hand-build a lines geometry through `packGeometry` with
your own index array.

`wireframeGeometry` and `edgesGeometry` landing proved the shape - a
builder returning a `"lines"` Geometry, no new object kind - and
[three-feature-survey](../notes/three-feature-survey.md) recorded the
family as the largest single untracked cluster in Three's surface. The
roadmap does not cover it at all.

## Where this sits

The standing three-way comparison, which decides the shape:

| | Three | Godot | Unity |
| --- | --- | --- | --- |
| Where helpers live | Runtime scene objects, `Object3D` subclasses | Editor gizmos only; runtime is an `ImmediateMesh` you fill, `Viewport.debug_draw` modes, `get_debug_mesh()` on shapes | Editor `Gizmos`/`Handles`, stripped from builds; `Debug.DrawLine` in play mode, editor views only; builds use `LineRenderer` or a `MeshTopology.Lines` mesh |
| Grid | `GridHelper(size, divisions, centerColor, gridColor)`, XZ | Editor viewport grid | Scene view grid |
| Axes | `AxesHelper(size)`, X red Y green Z blue | Editor origin gizmo, same colors | Scene view handle, same colors |
| Bounds box | `Box3Helper(box, color)`: a unit cube scaled by the transform; `BoxHelper(object)` live | `AABB` type, no drawer | `Bounds` type; `Gizmos.DrawWireCube(center, size)` |
| Plane | `PlaneHelper(plane, size, color)`: unit square, diagonals, normal tick, translucent fill child, placed by the transform | `Plane` type, no drawer | `Plane` type, no drawer |
| Vertex color on unlit | `LineBasicMaterial({ vertexColors })` | unshaded `StandardMaterial3D` + `vertex_color_use_as_albedo` | shader-specific; stock `Unlit/Color` ignores it |
| Line width | 1px; `Line2` addon | 1px; thick lines are meshes | `LineRenderer` quads |

Godot and Unity agree against Three on where helpers live, and the first
reason to side with Three is that we have no editor. The stronger one is
that Godot and Unity users reach for runtime equivalents anyway: it is
why the DebugDraw3D addon is one of Godot's most used and why Unity
ships `Debug.DrawLine` at all. Our leverage over all three is that
topology rides on the geometry, so the whole family is pure data:
tree-shakeable, zero cost unused, checkable headless. Three needs an
object type per helper because line-ness lives on the object. All three
engines are Y-up as we are, so the XZ grid and the axis colors are
unanimous; Unity's handedness changes nothing here.

The split by what a thing NEEDS: a grid, a triad, a box and a plane are
static data, so they are builders returning a Geometry, consistent with
`wireframeGeometry` and `edgesGeometry`. The live gizmos that have to
follow a node every frame (camera frustum, light cones, skeleton, vertex
normals) are components, and they are precisely the ones Godot and Unity
keep out of the runtime API; the Inspector app and the dev overlays are
the better home for those. Not this item.

## Shape

**Prerequisite: `vertexColors` on `unlit`.** An axis triad is three
colors and a grid is two-tone. `unlit` already draws lines (the topology
rides on the geometry) but has no `aColor` path at all: its vertex
template and fragment only know the instance color `iColor`, while `lit`
and `standard` take `vertexColors`. Mirroring `lit` is the option on
`UnlitOptions`, the class key, the glsl source option, `in vec4 aColor`
forwarded as `vColor` in the vertex stage (times `iColor` under
`instanceColors`), the multiply in the base sample, the cutout shadow's
read, and excluding the flag from `sprite`. About 20 to 30 mechanical
lines, not the one line the first draft claimed. It also serves any
hand-built colored geometry under an unlit material.

**Four builders** in `src/geometry.ts` beside `wireframeGeometry`, each
returning a `"lines"` Geometry, named after Three's helpers camelCased
(the rule `wireframeGeometry`/`edgesGeometry` already follow, so a Three
port finds them with one grep, and the four read as one family; the
first draft's bare `grid`/`axes`/`box3`/`planeOutline` invented two names
because `box` and `plane` are taken by the solid builders):

- `gridHelper({ size = 10, divisions = 10, color, centerColor })` - the
  XZ plane at y 0, Three's `GridHelper` with its defaults. Always the
  colored layout: the first draft emitted the standard layout when the two
  colors match, which makes the material depend on which options the
  caller passed to save a few hundred bytes on a debug grid. A material
  that does not read `aColor` draws colored geometry fine, so a
  single-tone grid under a plain `unlit({ color })` still works. The two
  lines through the origin take `centerColor`; they exist only when
  `divisions` is even.
- `axesHelper({ size = 1 })` - three segments from the origin, X red, Y
  green, Z blue. Colored layout.
- `box3Helper(bounds, options?)` - the twelve edges of the
  `[minX, minY, minZ, maxX, maxY, maxZ]` box that `geometryBounds`, a
  model's `bounds` and the spatial queries produce, Three's `Box3Helper`:
  8 vertices, 24 indices, standard layout. Three's helper geometry is a
  unit cube with the bounds in the transform, so the live box for one
  mesh is a node position and scale update over a static unit box, no
  rebuild - a doc line, not machinery.
- `planeHelper({ size = 1 })` - Three's `PlaneHelper` without its fill
  quad: the square outline, its diagonals and a unit segment along the
  normal. In the XY plane at the origin facing +z exactly like `plane()`,
  placed by the node transform. The first draft's
  `planeOutline({ size, constant })` was half a plane: there is no Plane
  type in the package and a constant without a normal places nothing.
  Three's own helper geometry is a unit square placed by the transform. A
  `{ normal, constant }` option lands additively once a Plane type exists.

Colors follow the material contract: sRGB 0..1 in, encoded to
premultiplied linear into `aColor`, which is what the shaders read
(`premultipliedColor` in color.ts, a pure module, so geometry.ts stays
pure and the check rig stays headless). All four take `GeometryOptions`
like every generator; the two colored ones default to the colored layout
and reject a custom layout without an `aColor` channel.

Placement: all four in `src/geometry.ts`, exported from `index.ts`. No
components, no intrinsic elements, no renderer changes, no Rust.

## Done looks like

Four builders exported and `unlit` taking `vertexColors`.
`checks/geometry-check.ts` covers each builder's vertex and index counts,
bounds, and the color channel where there is one - a builder is a pure
function, so the check rig is the right verifier and runs headless on
flux. `examples/wireframe.tsx`, already the lines example, gains a grid
and an axis triad under the rover and a bounds box around it, which puts
the two-tone and three-color paths on screen and makes them
snapshot-verifiable. A porter drops a grid and axes into a scene in two
lines, with no GLSL and no hand-built index arrays.

## Not in this item

- **The live gizmos**: `CameraHelper`, the directional/point/spot/
  hemisphere light helpers, `SkeletonHelper`, `VertexNormalsHelper`.
  Components that track a node every frame, and arguably Inspector and
  dev-overlay work rather than runtime API, per the comparison above.
- **`ArrowHelper`**. Three's is a line shaft plus a solid cone head.
  Topology rides on the Geometry and one mesh is one geometry and one
  material, so an arrow cannot be a single builder like the other four
  unless the head is lines too. Decided and landed the same day as
  `arrowHelper`: the shaft plus a pyramid-outline head, one lines
  geometry along +y, aimed by the node's rotation.
- **`BoxHelper`'s subtree half**. Three's `BoxHelper(object)` wraps an
  object's world bounds and refreshes them. Bounds here are per-mesh with
  no subtree world-bounds walk anywhere in the package, so wrapping a
  `Group` needs machinery that does not exist. `box3Helper` takes
  explicit bounds; the walk is separate work.
- **Line width**. 1px GL lines is the ES core guarantee, so a grid is
  thin and aliased on a HiDPI display however good the builders are.
  Three answers with the `Line2`/`LineMaterial` addon, quad expansion in
  the vertex stage. That is much larger than this item and it affects
  every lines geometry including `wireframeGeometry`, so it earns its own
  item if the helpers land and read too faint.
- **`PolarGridHelper`**. Trivial once `gridHelper` exists, and nothing
  has asked for it.
