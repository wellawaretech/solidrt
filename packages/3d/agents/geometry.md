# @solidrt/3d - geometry

Vertex layouts, indices and topology, the geometry generators, geometry as
data and the profile kit. Read ../AGENTS.md first: it carries the model
this file builds on.

## Geometry layout, indices and topology

One interleaved vertex buffer per geometry, described by an open layout
(`Geometry.layout`, absent = "base"): an ordered attribute list that
starts with `aPos` float32x3 and may carry any named channels after it,
each in a vertex format. Formats are WebGPU's spelling of (component
type, count, normalized): the float32 family (`float32` .. `float32x4`),
`float16x2/x4`, the normalized integers `unorm8x4`, `snorm8x4`,
`unorm16x2/x4`, `snorm16x2/x4`, the unsigned integers `uint8x4`,
`uint16x2/x4`, `uint32` .. `uint32x4` and the signed integers `sint8x4`,
`sint16x2/x4`, `sint32` .. `sint32x4`; `VERTEX_FORMATS` is the table.
Every format is a multiple of 4 bytes, so offsets and strides are
4-aligned by construction. A format feeds the shader `in` of its KIND
and component count, WebGPU's rule: float and normalized formats feed
`in vec4` (the fetch converts: `in vec4 aColor` reads a `unorm8x4` as
0..1), `uint*` formats feed `in uvec4` and `sint*` formats `in ivec4`,
exact at every width - so a per-vertex id or an index into a wide
table rides as `uint32` into `in uint aId`, and the "skinned" layout's
`aJoints` is `uint8x4` into `in uvec4 aJoints`. A layout that crosses
kinds (`uint8x4` into `in vec4`) is the missing-attribute error at
add() (`formatFeeds` is the rule). The base prefix `aPos` float32x3 + `aNormal`
float32x3 + `aUV` float32x2 is what every generator emits and what the
stock materials read, not a rule: a hand-built geometry declares what
it has, so a point cloud may carry `[aPos float32x3, aData float32]` at
16 bytes a point instead of 36, or a color as `unorm8x4` at 4 bytes
instead of 16. Pair a prefixless layout with a material that reads only
what it carries: `unlit` (aPos only) or a shaderMaterial; a stock lit
material over it throws the ordinary missing-attribute error at add().
A layout handed to a generator must start with the prefix and stay
float32-family (the generator writes floats); packing is a pass over
the result. `withAttribute(geometry, { name, format }, fill)` appends
one channel in any format (Three's `setAttribute` for an interleave;
the fill is given in float and encoded on write); "colored" names the
common case, the prefix plus `aColor` float32x4 - the per-vertex data
channel (a tint, baked AO, any four scalars; standard name, your
contents) - and `withColors(geometry, fill)` is its spelling. Fill is a
flat size-per-vertex array or a per-vertex callback receiving `(index,
pos, normal, uv)` (zeros for a channel the layout lacks). The fill is
raw floats: a tint the stock materials read under `vertexColors` is
premultiplied linear like every shader color, so encode an sRGB pick
with `premultipliedColor(color)`. `Geometry.vertices` is an
`ArrayBufferView` (a Float32Array for an all-float layout, a Uint8Array
when a channel is packed; never the contract): read and write a channel
through `geometryAttribute(geometry, name)`, an accessor whose
`get(i, k)`/`set(i, k, v)` speak floats whatever the format, and count
vertices with `vertexCount`. Materials read attributes BY NAME: a
material's vertex stage may declare any subset of its geometry's
channels, and a channel the program reads that the geometry lacks (by
name, or with a different component count) throws at add(). What a program reads is the ENGINE's word (`material.attributes()`
= `programAttributes` reflection of the linked program, instance
attributes excluded), not a parse of the GLSL: an `in` the compiler
dropped does not count, and the engine also rejects a pipeline whose
attribute lists leave a read attribute uncovered. The material
keeps one program and builds one pipeline per layout its meshes bring,
so a geometry may carry more than a material reads. The whole layout
ships whether a material reads every attribute or not (inactive
attributes only keep the stride), so extra channels cost their bytes on
every draw of that geometry - keep data-light passes (a wireframe
reading only aPos) on base-layout geometry. `layoutStride` (bytes)/
`layoutSlot` (byte offset, format, components)/`layoutKey`/
`layoutAttributes` are the layout arithmetic; two layouts with equal
keys interleave identically (merge requires that).
STREAMS: a geometry may carry more than one vertex buffer.
`Geometry.vertices` under `layout` is stream 0 (where aPos lives) and
`Geometry.streams` holds extra `{ layout, vertices }` buffers, each with
its own attribute list and the same vertex count (Unity's vertex
streams; Three's one buffer per channel when a stream carries one
attribute). `withAttribute(geometry, attr, fill, { stream })` appends a
channel to stream 1..n or opens stream n + 1; `geometryAttribute`,
`geometrySlot`, `geometryKey`, `geometryLayouts`, `geometryStreams`
and `geometryVertexCount` read across streams, and a material's
pipeline is one per `geometryKey` (every stream's layout, `|` between
them). Streams exist for data on its own schedule: `updateVertices(
geometry, { stream?, first?, count? })` re-uploads that vertex range of
that stream from the geometry's own array in place (write the array
through the accessor first; Three's `needsUpdate` with an update
range), reaching every mesh, view and wireframe over the geometry
while the other streams never move; a stream-0 update drops the cached
bounds and rewrites the same range of the core's copy of the positions,
so the mesh picks and culls where it is now drawn (the box follows at
the next flush, the triangle index is rebuilt by the next query - a
deforming mesh needs no re-attach). transformGeometry and
the edge builders share extra streams with their source; merge
concatenates every stream; the `.sol3m` container refuses streams (run
time data). `setDrawRange(mesh, first, count?)` draws indices `[first,
first + count)` of the mesh's geometry (Three's `setDrawRange`, on the
mesh because the scene owns the entries), applied to every entry of the
mesh and reset by setGeometry. The worked example is
`examples/streams.tsx`.
Indices are uint16 or uint32 - the `Geometry.indices` array type picks
the draw's index format, so hand-built geometry past 64k vertices just
uses a Uint32Array (generators emit uint16). What the indices LIST is
`Geometry.topology` (`"triangles"` when absent; `"lines"`,
`"line-strip"`, `"points"`, `"triangle-strip"`): the index buffer and
its primitive travel together (Godot's surface primitive, Unity's
SetIndices topology), a material builds one pipeline per (layout,
topology) pair its meshes bring, and validateGeometry enforces the
count rule at add() (a whole number of triangles or lines, one
primitive's worth for a strip). Materials have no topology of their
own. Only a triangle list gets a triangle narrowphase; lines and points
pick and collide by their bounds box (which follows `updateVertices` all
the same), and cast no shadow. `geometryTopology`
reads the field with the default applied. POINTS: a point cloud is
ordinary indexed geometry - one vertex per point, the index buffer
listing them, `topology: "points"`, no constraint on the index count.
Under `"points"` the vertex stage writes `gl_PointSize` (honored to the
pixel; `size / gl_Position.w` attenuates it with distance) and the
fragment reads `gl_PointCoord` (0..1 across the sprite) to shape the
splat - `discard` outside the inscribed circle for round ones. The
worked example is `@solidrt/core`'s `examples/gpu-particles.tsx`. Do
NOT reach for instancing to draw a cloud (Instancing in instancing.md has the
cost model). Geometry GPU buffers are lazy, shared, and
reference-counted by draw entries: removing the last
entry frees them at the end of the microtask (a same-tick rebuild keeps
the upload), so swapping `<Mesh geometry>` reactively never accumulates
old generations; the vertex upload is keyed on the vertices view
itself, so geometries sharing a vertex array (a wireframe over its
source) share one buffer. `disposeGeometry` is the immediate explicit
free.

## Geometry generators

Geometry generators take ONE options object, every field optional with
a default, named as Three names them: `box({ width, height, depth })`
(1x1x1); `plane({ width, height })`, `circle({ radius, segments })` and
`ring({ innerRadius, outerRadius, segments })` (XY, facing +z - rotate
`[-Math.PI/2, 0, 0]` for a floor); `sphere({ radius, widthSegments,
heightSegments })`; `cylinder({ radiusTop, radiusBottom, height,
radialSegments, heightSegments })` (y axis, capped; unequal radii taper
it) and `cone({ radius, height, radialSegments, heightSegments })`;
`capsule({ radius, height, capSegments, radialSegments, heightSegments
})` (y axis, `height` the TOTAL extent like
cylinder's and Godot's/Unity's where Three's is the middle section
only, so `height: 2 * radius` is a sphere and less throws; its collision
volume is `{ a: [0, -(height / 2 - radius), 0], b: [0, height / 2 -
radius, 0], radius }`); `torus({ radius, tube,
radialSegments, tubularSegments })` (lying flat, hole on the y axis) and
`torusKnot({ radius, tube, tubularSegments, radialSegments, p, q })`
(standing y-up) - both oriented for the y-up world, unlike Three's z-up.
No positional form: `box()` is the default cube, `box({ label: "rock" })`
names it. The polyhedra: `tetrahedron`, `octahedron`, `icosahedron` and
`dodecahedron` (`{ radius, detail }`, radius 0.5 like sphere where
Three's is 1) and the generic `polyhedron(vertices, indices, { radius,
detail })` over Three's data form (flat xyz list, CCW triangle list, the
data first like `box3Helper`), each solid projected onto its
circumsphere. `detail` splits every edge `detail + 1` ways first, so
`detail: 0` is the flat-shaded solid (face normals) and anything above a
sphere of uniform triangles with radial normals: `icosahedron({ detail:
3 })` is the icosphere, no pole pinch. Non-indexed like Three, every
triangle owning its three vertices; `edgesGeometry` welds by position,
so `edgesGeometry(dodecahedron())` is the twelve pentagons (the fan
diagonals are coplanar and drop out) where `wireframeGeometry` shows the
triangulation. UVs are the spherical map with Three's seam patch,
stretched toward the poles: the textured sphere stays `sphere()`, the
polyhedra are for flat-shaded, low-poly and procedurally shaded looks
(examples/polyhedra.tsx). Every options object (the profile kit's `extrude`/`lathe`/
`sweep`/`tube` too) also takes `label` and `layout` - `layout` makes the
generator emit that layout in one pass (base channels written, the
extra slots zero), so `box({ layout: "colored" })` then
`fillColors(g, fill)` builds colored geometry without the
generate-then-repack copy; the result is byte-identical to
`withColors(box(), fill)`. `packGeometry(verts, indices, options?)` is
the tail every generator ends in, for your own generators.
`withAttribute(geometry, attr, fill, label?)` derives a copy of any
geometry (generator or hand-built) with one more channel after its
current layout; the source is untouched. `withColors(geometry, fill,
label?)` is the aColor float32x4 case, keeping the "colored" preset
name (`withAttribute` with `unorm8x4` is the 4-byte color).
`fillAttribute(geometry, name, fill, first?, count?)` is the in-place
primitive under both: overwrites one channel the geometry's layout
already carries (withAttribute ADDS one), reading pos/normal/uv from the
buffer itself - so a builder that bakes transforms while writing hands
the baker world-space vertices. `fill` indexes relative to `first`.
`fillColors(geometry, fill, first?, count?)` is its aColor spelling.

## Geometry as data

Geometry as data: `transformGeometry(geometry, { position?, rotation?,
quaternion?, scale? }, label?)` bakes a placement into a copy (the
setTransform shape: Euler XYZ radians or a quaternion, number = uniform
scale), positions through the matrix and normals through its
inverse-transpose, renormalized - correct under non-uniform scale; uvs,
colors, indices and layout copy through. `mergeGeometries(parts, label?)`
concatenates parts into one geometry with offset indices (uint32 past 64k
vertices); parts must share one layout, a mixed list throws. Together
they collapse a static scene to one mesh per material - transform each
part into place, merge, draw once - so only what actually moves keeps a
node, a draw entry and a per-frame `uModel` write of its own. Merging
does not tax picking: a merged geometry's raycast narrowphase runs
through its triangle BVH (built on the first ray), so merge for draw
count without giving up ground queries. Both are
pure array math (Three's `applyMatrix4` + `mergeGeometries`), no GPU
call, and the source geometries are untouched. `wireframeGeometry(geometry,
label?)` and `edgesGeometry(geometry, thresholdAngle?, label?)` (Three's
WireframeGeometry/EdgesGeometry) build `"lines"` geometry over a triangle
geometry's OWN vertex array and layout, shared by reference: every edge
once for the wireframe, for the edges only those where two faces meet at
the threshold (degrees, default 1) or more plus the open borders. Edges
are matched by position, so a uv seam or a per-face normal split draws
one line; a generated round shape is faceted and keeps its facet lines
until the threshold passes 360 / radialSegments. The wireframe of a
"skinned" part draws in its pose under `unlit({ skinned: true })` with
the mesh's palette, and the swap onto a live mesh is
`setGeometry` + `setMaterial` (`examples/wireframe.tsx`). Lines are one
pixel wide on GL ES; thick lines are quad geometry. The debug helpers are
`"lines"` builders too, Three's helper classes camelCased:
`gridHelper({ size?, divisions?, color?, centerColor? })` is the XZ grid
at y 0 (defaults 10 by 10, Three's grays; the two lines through the
origin take `centerColor` and exist only for an even `divisions`),
`axesHelper({ size? })` the X red, Y green, Z blue triad from the origin,
`box3Helper(bounds, options?)` the twelve edges of a
`[minX, minY, minZ, maxX, maxY, maxZ]` box in local space, and
`planeHelper({ size? })` a square outline with its diagonals and a unit
normal tick in the XY plane facing +z, placed like `plane()`, and
`arrowHelper({ length?, headLength?, headWidth? })` an arrow along +y
with a pyramid-outline head, aimed by the node's rotation
(`quatFromTo(out, [0, 1, 0], direction)`), and `capsuleHelper(volume,
{ segments? })` the outline of a `{ a, b, radius }` collision capsule
(rings at both ends, four lines, two half circles per cap; `a == b`
draws a sphere's three great circles) in the volume's own space, the
gizmo Unity and Godot draw for a capsule collider. Godot and
Unity keep these in the editor and Three makes them scene objects; here
they are plain geometry a node places and a material draws. The grid and
the triad are "colored" layout (sRGB colors in, premultiplied linear
aColor out, the material contract) drawn by `unlit({ vertexColors:
true })`; a material that reads no aColor draws them in its own color.
The box, the plane, the arrow and the capsule are base layout. For a bounds box that moves
every frame, draw `edgesGeometry(box())` on a node whose position is the
box center and whose scale is its size and update the transform (Three's
Box3Helper does exactly that) instead of rebuilding.
`examples/wireframe.tsx` draws a grid, the triad and the rover's bounds
box. `geometryBounds(geometry)`
returns the cached local AABB `[minX, minY, minZ, maxX, maxY, maxZ]`, and
`rayBoxDistance(ox, oy, oz, dx, dy, dz, minX, .., maxZ)` is the picking
slab test (entry t >= 0 in units of the direction's length, 0 from
inside, -1 for a miss) - for ray-testing boxes you keep yourself
(triggers, collision volumes) without meshes you do not want to draw.

Normals as data: two ops, split by whether the vertex count survives.
`computeVertexNormals(geometry)` recomputes a triangle geometry's
normals IN PLACE from the faces that name each vertex by index (Three's
`computeVertexNormals`, Unity's `RecalculateNormals`, Godot's
`generate_normals`, all in place), each face weighted by its corner
angle so the result does not depend on how a surface was triangulated.
The math runs in the core in one call over the vertex floats (so aNormal
must be float32x3; a packed normal is an authoring format, pack after
withNormals), and it returns the stream carrying aNormal, which
`updateVertices` re-uploads: the CPU deformation loop is
`fillAttribute(g, "aPos", ...)`, `computeVertexNormals(g)`,
`updateVertices(g)`. That loop is for data that genuinely changes on the
CPU - a streamed cloud, a cloth solved in JS, an editor edit. A
per-frame ripple, sway or wave belongs in a vertex shader: displace and
derive the normal in the vertex stage (`shaderMaterial`), and no
per-vertex work touches the CPU at all; the fillAttribute callback alone
is an interpreter call per vertex per frame. A vertex shared across faces
shades smooth and a split one per face, so a merged or deformed
generator geometry comes out right; a uv seam's copies each see one side
and the seam shows (Three's artifact too). `withNormals(
geometry, creaseAngle = 60, label?)` is the authoring path and a COPY:
faces are matched by position, so a triangle soup, a hand-written face
list, a merged result or a geometry with no aNormal channel at all (one
is added, float32x3) all work and a seam shades smooth across it; 0 is
flat shading, 180 smooths everything, 60 (Three's `toCreasedNormals`
and Unity's import smoothing angle) keeps hard edges hard. It is
`toNonIndexed` + per-corner normals + `mergeVertices`, so the result is
indexed and split only where a crease or a seam needs it (a box stays
24 vertices under 90 degrees). Both pieces are exported:
`toNonIndexed(geometry, label?)` gives every index its own vertex
(identity indices; per-face colors via fillAttribute, flat shading via
computeVertexNormals), `mergeVertices(geometry, tolerance = 1e-4,
label?)` welds vertices equal in every channel back together, the exact
inverse (a normal or uv that differs keeps them apart). Neither takes a
morphed geometry: split or weld before `withMorphTargets`.
`normalsHelper(geometry, { size?, color? })` draws one line per vertex
along its normal (Three's VertexNormalsHelper as a static builder,
"colored" layout, red by default, drawn by `unlit({ vertexColors: true
})` under the mesh's node), the way to look at any of this;
`examples/normals.tsx` shows the three crease angles on one soup, a
rippling sphere on the per-frame loop, and the helper refreshed in place
with the same loop. Three's `center()` is `transformGeometry(g, {
position: [-(b[0] + b[3]) / 2, -(b[1] + b[4]) / 2, -(b[2] + b[5]) / 2]
})` over `geometryBounds`; there is no bounding sphere op, the box is
what every query and the LOD read.

## Profile kit

Profile kit (2D outlines to solids, real texture UVs): a `Profile` is a
closed XY polygon, bare `[x, y]` points crease, `{ p, smooth }` points
share an averaged normal - `fillet(points, radius, segs?)` and
`roundRect(w?, h?, radius?, segs?)` emit those (arc corners smooth).
Winding is normalized, so either authoring direction works.
`extrude(profile, { depth, bevel, bevelSegments })` sweeps along z,
centered, with a quarter-round bevel at both rims; `lathe(profile, {
segments, angle, start })` revolves a CLOSED (x = radius, y = height) profile about the y
axis - watertight by construction, flat caps on partial sweeps;
`sweep(profile, path, options?)` runs the profile along an open 3D polyline with
MITRED joints (each cross-section sits on its bend's bisector plane, so
bends never gape or overlap) and flat caps at both ends. The path
mirrors the profile convention: bare `[x, y, z]` points crease (a strap
folding over an edge), `{ p, smooth }` points shade continuous (tag a
sampled curve's points); the profile's y starts as close to world up as
the first segment allows, then parallel-transports without spinning.
Closed loops are NOT supported yet - overlap the ends by a segment to
fake one. `tube(path, { radius, radialSegments })` is the round-profile
shorthand (wire, rope, pipe), and `pathFrames(path)` exports the
per-segment frames (tangents, cross-section axes, arc lengths) for
custom work along a path. `polygon(profile, options?)` fills one flat (facing
+z, like circle); `triangulate(points)` is the ear-clipping core (fan
fallback, never drops a cap), exported for custom flat work. These pick
uint16/uint32 indices by vertex count automatically.

## Traps

- Per-generator conventions - orientation, UV mapping, which axis a solid
  stands on, what a cap looks like - live on each generator's doc comment,
  not here. They are consistent (`plane`/`circle`/`ring` face +z, `torus`
  lies flat with the hole on y, discs and cylinder caps get a PLANAR disc
  map inscribed in the unit square) but the doc comment is the source.
- Entry rebuild order: `setGeometry`/`setMaterial` re-add the entry at the
  list END and rebind it with the new material's key, so the core's next
  flush re-sorts and the mesh keeps its place.
- `lathe` takes a CLOSED profile (a cross-section with thickness, or run
  to the axis at x = 0) - it is a solid of revolution, NOT Three's open
  polyline shell. An "open" outline must be closed by the author;
  otherwise the shape is simply wrong, there is no open-profile mode.
- `useScene()`/`Group`/`Mesh` throw outside `<Scene>` (default-less
  context).
- Geometry local bounds cache on the Geometry (like its GPU buffers and
  the core's picking copy): mutating `vertices` after a mesh used them
  leaves stale bounds, a stale GPU buffer AND a stale picking shape
  until `updateVertices` publishes the range - write through the
  accessor and call it, or make a new Geometry.
