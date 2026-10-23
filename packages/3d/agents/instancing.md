# @solidrt/3d - instancing

Instanced and record meshes, instance order and visibility. Read
../AGENTS.md first: it carries the model this file builds on.

## Instancing

Instancing - one draw entry covering a population, in two forms. The
axis between them is WHERE MOTION IS COMPUTED, the same split as
@solidrt/2d's sprite and record layers.

FIRST, whether to instance at all. An instance costs the GPU a fixed
per-instance setup, so records buy cheap MOTION, not cheap VERTICES:
below roughly a hundred vertices per instance that setup dominates, and
a plain geometry with a bigger vertex buffer draws faster. A point cloud
is the extreme case - one vertex per point in an ordinary indexed
`topology: "points"` geometry, no instancing, measured at 4x the frame
rate of the same points as records. Packing more points per record does
not recover it: every vertex of an instance fetches ALL of that
instance's attributes, so attribute bandwidth grows with the packing.
Reach for instancing when each instance is a real object (a ship, a
tree, a crowd member), not to submit a lot of vertices. The tell in
`/gpu` is a large `instanceCount` beside a tiny `indexCount`.

### createInstancedMesh

`createInstancedMesh(geometry, material, { capacity?, bounds?, anchor?, label? })`
draws the geometry once per instance NODE: `addInstance(mesh,
transform?, parent?)` returns a scene node (kind "instance") placed
inside the mesh - setTransform/setTransition/setVisible, lookAt,
worldPosition, pointer handlers and children of its own all apply - whose
matrix RELATIVE to the mesh the spatial core writes into the record
buffer (`bindMatrixRecord` anchored on the mesh node: one coalesced
buffer write per flush however many instances moved, so native
transitions and clip players move instances with zero per-frame JS).
`anchor` moves that frame to an ANCESTOR the mesh sits under at
identity: the records are then relative to the anchor and instances may
be placed under any node of the anchor's subtree, not only under the
mesh - a population whose copies ride a hierarchy the mesh is not the
root of (createModel's shared parts: the mesh at identity under the
model root, an instance under every placement node). The anchor must be
an ancestor when the mesh is added (it throws otherwise); keeping the
chain between them identity is the caller's contract, the same one an
instanced LOD's levels live under.
The material declares `INSTANCE_MATRIX_ATTRIBUTES` as its first instance
buffer (four vec4 columns, 16 floats; anything else throws at creation)
and its vertex
stage splices `INSTANCE_MATRIX` from `@solidrt/3d/glsl`: `uModel *
instanceMatrix() * vec4(aPos, 1.0)`, normals through
`instanceNormalMatrix()` (Three's derivation, exact for any rotation and
scale at no per-instance data; a sheared hierarchy - a non-uniformly
scaled group above a rotated instance - takes
`transpose(inverse(mat3(instanceMatrix())))` instead) - or is a stock
material with `instanced`. A second instance buffer, when the material
declares one, gives every instance a STYLE record beside its matrix
(`MeshInstances.streams[0]`): an app-owned stream in the material's
layout, written per instance with `setInstanceStyle(instance, values)`
(one value per attribute component, encoded by the codecs) or by name
through `instanceAttribute` + `updateRecords`, into a JS mirror and
published as ONE coalesced buffer write per stream at the scene's sync
(a frame-rate path, like setTransform); a fresh or recycled slot starts
from the material's `instanceStyle` (white under `instanceColors`, zeros
otherwise); a third buffer is a further stream, reached by name only;
under `alphaTest` the cutout shadow
multiplies by the same color, so an instance faded below the cutoff
casts nothing, like its pixels (Godot's alpha scissor holds in its
shadow pass too). The core's pose buffer and the app's style buffer are
@solidrt/2d's split one dimension up. Slots are fixed
for an instance's life and recycle on `destroy` (at the free: an
instance cannot exist outside its mesh, so the generic add/remove throw
on one), the drawn count is the slot high-water mark,
and past `capacity` (default 64) every buffer doubles into a
replacement, with the live matrix records retargeted in one core call
and each stream's mirror republished. `parent` may be a group inside the mesh's subtree (a squad
in a fleet): the record stays mesh-relative through it. Picking is per
instance: every instance node carries the geometry's bounds and
triangle shape, so `Hit`, `Overlap` and `Impact` name the `instance` and
pointer events bubble from it (`event.instance`) through the mesh to its
ancestors. `bounds` on the mesh is optional here: without it the mesh
culls by the union of its live instances' boxes (following them, hidden
ones included) and sorts by its node position; with it the explicit
box does both. Three's InstancedMesh count
constructor over Unity's one-transform-per-instance model; Godot's
MultiMesh is the record form. The components: `<InstancedMesh capacity>`
with `<Instance position style transition onPointerDown>` children
(`<Group>` children are squads, an `<Instance>` is a parent too);
`examples/fleet.tsx` is a thousand of them springing between formations
with tap-to-tint.
Mount cost is the one difference between the faces: an `<Instance>` row
costs about eight times an `addInstance` call (the component machinery -
a Solid row, two effects, the style effect; a node component's context
provider is built only when it has children, the biggest single
saving), so a few thousand
mount in tens of milliseconds either way; populations spawned per frame,
or in five figures, belong to the function face.

### createRecordMesh

`createRecordMesh(geometry, material, records, count?, { bounds?,
instanceOrder?, label? })` is the raw form: `records` is the
per-instance data laid out
in the material's first instance layout - a Float32Array over an
all-float layout, otherwise bytes built with `vertexView(layout, buffer)`
and `attributeAccess` like a geometry stream (a length that is not whole
records throws) - copied into the first stream's mirror, whose capacity
starts at the records given; every further instance buffer the material
declares is a zeroed stream of the same capacity, reached by name
through `instanceAttribute`. `count` picks how many draw (default all).
`setRecords(mesh, records, count?)` rewrites from the start (count
defaults to the records written; more than capacity GROWS: capacity
doubles into replacement buffers, the entry is re-pointed via
`setDrawBuffers`, the old ones are freed), `records(mesh)` (the mirror)
or `instanceAttribute` + `updateRecords(mesh, { first, count })`
rewrites a few (the range is against capacity, so write ahead and dial
after), and `setRecordCount(mesh, n)` is the population dial (clamped to
capacity; frame-rate-safe). The @solidrt/2d records layer speaks the
same three verbs over its sprites, `setRecords` alone staying the
mesh's (a layer's population is addSprite's). Records are opaque data (position/yaw/tint/whatever
your shader reads), so the
library cannot know where they place instances: a record mesh has NO
picking leaf unless you pass `bounds` (local, covering the population) -
then it picks and transparent-sorts conservatively as one box. The
escape hatch for motion only JS can compute at scale (a particle sim, a
crowd stepped in a worker).

### Common to both

Everything mesh works on both: setTransform moves the whole population
through one uModel, setVisible zeroes the drawn count and restores it on
unhide, renderOrder/params/geometry/material swaps apply, and
`disposeInstances(mesh)` detaches and frees the record buffers - the one
explicit free, geometry-buffer rule. `examples/fleet.tsx` (instances,
components) and `examples/instanced.tsx` (records) are the live proofs.

### Instance order

`instanceOrder` on either create (and the components) draws a
population's records in key order, decided core-side; slots as you write
them never move. `{ position: "iCenter", retain: true, descending: true
}` names a float32x3/x4 attribute holding a local-space position: records
draw by their depth along the scene camera's view direction, which the
SCENE feeds the core whenever the camera or the mesh turns meaningfully
(mapped through the mesh's rotation; an epsilon gate meters the
re-sorts) - back-to-front transparency over 100k+ records with zero
per-frame JS, and with `retain` a camera turn re-sorts and republishes
core-side while a parked camera uploads nothing. `{ field: "age" }`
names a float32 attribute and draws by its value, re-ordered when the
records publish. The costs, by design: an ordered mesh publishes the
LIVE record set whole on any write (a byte range has no stable position
under a permutation; `count` defines the sorted population), and one
buffer holds ONE order - the scene target's entry owns it, views and
shadow passes draw the same gathered buffers, and a mesh layered out of
the scene target keeps its last order. Where the key sits decides
nothing else: raw instancing stays buffer-order everywhere (Three,
Unity, Godot), so this knob has no engine counterpart to compare
against; the core primitive is okf/done/gpu-instance-order.md.

`transfer: true` (createRecordMesh only) is the write-once form for a
large population: the records go to the ENGINE at creation and the mesh
keeps no JS mirror, so the record bytes are resident once CPU-side
(engine mirror) plus the GPU buffer, instead of twice more (JS mirror,
plus the creation bytes the app would keep). The contract is loud:
setRecords, updateRecords, instanceAttribute and growth throw naming
the transfer; setRecordCount still works (with an instanceOrder the
engine re-sorts and republishes the first n records of ITS copy),
and so do setVisible, transforms, re-attach and disposeInstances (which
frees the engine copy with the buffer). Takes a single-stream material;
with an instanceOrder the engine's retained copy is implied. The engine
mirror is BUFFER-lifetime, so a detached-and-re-attached ordered entry
(setLayers re-admission included) seeds its order from it with no
publish from JS - true for every retained ordered mesh since this
landed, transfer or not.

The INDEXED form, `instanceOrder: { position: "iCenter", records:
LAYOUT, descending: true }` (createRecordMesh only), is for a population
whose records live in a DATA TEXTURE the vertex stage fetches by record
index - a splat cloud, any large write-once set whose per-instance
frontend cost matters. `records` is the layout of the record bytes you
pass: the engine keeps their key column (the position, 12 bytes per
record; 8 packed) as its sort mirror and nothing else, and the material's ONE
instance buffer is the id stream: uint32/uint32x4 attributes the engine fills
with the sorted record indices, K ids per instance record, so one drawn
instance covers K records - repeat the geometry K times and pick the
record with `gl_VertexID` (quad `gl_VertexID / 4`), fetch it by id
(`texelFetch` on an `rgba32ui` texture the app built from the same
bytes and bound per mesh), and cull the id 0xFFFFFFFF that pads the
last group. Transfer is implied (the engine reads its key column off
the records at creation), a re-sort uploads 4 bytes per record instead of the
record, and `count` stays in records while the entry draws
ceil(count / K) instances (`drawCount`). `MeshInstances.textures` holds
data textures the population owns, freed by disposeInstances.

## Traps

- `visible: false` keeps the entry, drawn with `instanceCount: 0` (a
  cheap off switch); unhiding writes 1, or the mesh's own record count
  when it is instanced - never a bare 1 into an instanced entry. Hidden
  meshes skip uModel writes; the fresh matrix is
  written on unhide. A freshly attached entry starts off the same way and
  the core's flush turns it on when it writes uModel - never add one
  live: it has no world matrix yet, and drawn before the sync (which runs
  ahead of the paint for a write made in a frame, at the microtask
  otherwise) it would flash at the world origin for a frame.
- Instancing pairs strictly at add(), like layout: an instanced material
  needs a createInstancedMesh or createRecordMesh mesh and vice versa,
  and every instance buffer the material declares must be a layout the
  mesh carries - the matrix (INSTANCE_MATRIX_ATTRIBUTES, the first
  buffer of an instanced mesh) or a stream with the same attribute names
  AND formats (a byte-equal layout in other formats decodes differently)
  - each mismatch throws there, at creation for the mesh's own material
  and at add() for a swapped one. The instance buffers are MESH-owned
  (unlike shared geometry buffers): `disposeInstances` is their one
  free, and the mesh cannot be re-added afterwards. Capacity grows by
  REPLACEMENT, never resize: an addInstance or setRecords past capacity
  doubles into new buffers and swaps them in (an instanced mesh's live
  matrix records move in one core `retargetRecords`, every stream's
  mirror republishes) - amortized like a dynamic array, same policy as
  @solidrt/2d; size `capacity` or the initial records to skip the
  copies. A mesh's stream layouts come from its material AT CREATION: a
  later setMaterial to a class with another layout throws at the
  rebuild.
- Record writes are mirrored, not immediate: `setInstanceStyle`,
  `setRecords` and an `instanceAttribute` write land in the stream's
  `data` (the core record stream's mirror) and the scene's sync publishes
  its dirty range (`updateRecords` marks it for accessor writes) - so a write on a mesh outside any scene
  shows once it is added (the dirty range waits), and reading the GPU
  buffer back between the write and the sync lags the mirror. The sync
  runs in the publish pass of core's `onBeforeRender`, ahead of the
  frame's paint (at the microtask for a write made outside a frame), so
  a `setRecords` made in onFrame is in that frame's picture with its
  count, its buffers and its bytes together - the count alone
  (`setRecordCount`) reaches the engine at once. Growth replaces
  `data`: hold the accessor, or re-read `records(mesh)` at use time,
  never a hoisted view.
- The sugar is for FEW records, the mirror is for MANY. `setInstanceStyle`
  over an all-float layout is indexed stores; over a packed layout (the
  stock `float16x4` color included) it is a codec call per component,
  and so is every accessor write. Measured at ten thousand instances
  restyled every frame on a laptop under QuickJS: about 11 ms of JS per
  frame through setInstanceStyle on a float layout, 19 ms on the half
  color, 9 ms through the accessor, 5 ms through the mirror - and that
  5 ms is the app's own per-instance tint math, the write itself is
  free. A population restyled per frame writes the mirror in bulk:
  `records(mesh)` (a Float32Array over an all-float layout, bytes
  otherwise - a `Float16Array` over the stock color, indexed by slot
  times components) filled in a plain loop, then ONE `updateRecords(mesh,
  { first, count })` - zero per-component calls, the same shape as
  `setRecords`.
- Instanced casters: `castShadow` on a populated mesh needs a depth pass
  with the instance placement in it - the stock materials' `instanced`
  carries one, a custom class needs `shadowVertex` (see the traps in lighting.md);
  a class without one is skipped by shadow views, silently.
- A record mesh without explicit `bounds` has no BVH leaf: it never
  picks, pointer events never target it, and its transparent sort key
  falls back to the node's world position. That is deliberate - records
  are opaque to the library, so any inferred box would be a guess. Supply
  `bounds` for anything pickable or transparent. An instanced mesh picks
  per instance regardless and culls by its instances' union; its
  `bounds` replace that union and give the transparent sort a center.
- An instance node's own fields hold its LOCAL pose (relative to its
  parent, as for any node); the record the core writes is its pose
  relative to the MESH, composed through any group between them. A
  destroyed instance takes its children with it, like any destroyed
  node. `disposeInstances` on an instanced mesh flushes the
  core before freeing the buffer, so the destroyed instances' hiding
  writes land in a live buffer; keep that order if you ever free a
  record buffer by hand.
- An anchored population (`createInstancedMesh` with `anchor`) draws
  `uModel * record`, and the record is relative to the ANCHOR: the
  copies land right only while the mesh's world equals the anchor's.
  Moving the mesh itself (or any group between it and the anchor) shifts
  every copy twice; move the anchor, the instances or their parents
  instead. A model's shared parts are anchored this way - `setTransform`
  on `model.parts[i].mesh` of a part with `instances` is the mistake.
