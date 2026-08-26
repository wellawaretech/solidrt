# @solidrt/3d - agent notes

A retained 3D scene graph above `@solidrt/core/gpu`. Meshes, materials and
a camera compile to ONE depth-buffered draw target (`createDrawTarget` +
one `addDraw` entry per mesh); the scene's output is an ordinary texture
id composited as a `<texture>` leaf, so it takes layout, transforms,
blendMode and pointer events like any element.

## The model

- Two layers. The imperative core is Solid-free: `createScene`,
  `createMesh(geometry, material)`, `add`/`remove`, `setTransform`,
  `lookAt`, `getRotation`, `setVisible`, `setRenderOrder` - plain objects
  over the spatial core (`flux:spatial`, `alloy/src/spatial/`): every node
  in a scene has a core node, JS keeps the LOCAL position/quaternion/scale
  as the readable truth and forwards each write, and the core's flush
  (one call per microtask) recomputes only the moved subtrees and writes
  each entry's uModel (plus uNormal for materials declaring it) and its
  visibility switch itself - a move costs its subtree, never the scene.
  ONE `setTargetParams` (the shared uViewProj + uCamPos) per camera
  change, however many meshes. World matrices live in the core only:
  `worldPosition`/`lookAt`/picking read them back (`worldMatrix`, pending
  writes included). See okf/backlog/spatial-core.md for what still runs
  in JS and why. The component face (`Scene`/`Group`/`Mesh`/
  `PerspectiveCamera`) syncs props into that core over context and renders
  nothing itself.
- Rendering is the runtime's. The target is `render: "auto"`: it
  re-renders when entries change, so a STATIC scene costs zero passes and
  the library registers no frame loop. Continuous animation is the app's
  own `onFrame` writing a signal (declarative) or `setTransform` on a
  `ref`-grabbed node (the frame-rate escape hatch - signals carry
  structure, per-frame motion goes straight to the scene).
- RETARGETED motion is native: `setTransition(node, { position:
  { duration: 400 }, ... })` makes setTransform writes TARGETS the core
  animates toward every frame (position/scale per lane, rotation along
  the quaternion geodesic - a spring keeps its velocity through
  retargets), so a mesh gliding to a slot or a camera rig easing costs
  one JS write per target change, zero per frame. The declaration lives
  on the SceneNode and re-applies on every scene enter; the pose a node
  enters with always snaps. Each natural settle calls the node's
  `onTransitionEnd` (plain field like the pointer handlers) with
  `{ component }`; the raw "spatialTransitionEnd" engine event
  (srt:events, carrying the CORE node id `_node`) stays for flux:spatial
  consumers.
- One interleaved vertex buffer per geometry, described by an open layout
  (`Geometry.layout`, absent = "standard"): an ordered attribute list that
  always starts with the standard prefix `aPos` vec3 + `aNormal` vec3 +
  `aUV` vec2 (what every generator emits) and may carry any named channels
  after it. `withAttribute(geometry, { name, format }, fill)` appends one
  (Three's `setAttribute` for an interleave); "colored" names the common
  case, the prefix plus `aColor` vec4 - the per-vertex data channel (a
  tint, baked AO, any four scalars; standard name, your contents) - and
  `withColors(geometry, fill)` is its spelling. Fill is a flat
  size-per-vertex array or a per-vertex callback receiving `(index, pos,
  normal, uv)`. Materials read attributes BY NAME: a material's vertex
  stage may declare any subset of its geometry's channels, and a channel
  the program reads that the geometry lacks (name + format) throws at
  add(). What a program reads is the ENGINE's word (`material.attributes()`
  = `programAttributes` reflection of the linked program, instance
  attributes excluded), not a parse of the GLSL: an `in` the compiler
  dropped does not count, and the engine also rejects a pipeline whose
  attribute lists leave a read attribute uncovered. The material
  keeps one program and builds one pipeline per layout its meshes bring,
  so a geometry may carry more than a material reads. The whole layout
  ships whether a material reads every attribute or not (inactive
  attributes only keep the stride), so extra channels cost their floats on
  every draw of that geometry - keep data-light passes (a wireframe
  reading only aPos) on standard geometry. `layoutStride`/`layoutSlot`/
  `layoutKey`/`layoutAttributes` are the layout arithmetic; two layouts
  with equal keys interleave identically (merge requires that).
  Indices are uint16 or uint32 - the `Geometry.indices` array type picks
  the draw's index format, so hand-built geometry past 64k vertices just
  uses a Uint32Array (generators emit uint16). Geometry GPU buffers are
  lazy, shared, and reference-counted by draw entries: removing the last
  entry frees them at the end of the microtask (a same-tick rebuild keeps
  the upload), so swapping `<Mesh geometry>` reactively never accumulates
  old generations. `disposeGeometry` is the immediate explicit free.
- Materials dedupe hard: one program + one pipeline per material CLASS
  (unlit color, unlit map, each opaque or transparent), `depth: true` +
  `cull: "back"`; an instance is
  just per-entry uniforms (`uColor`) and bindings (`uMap`).
- The pure pieces (`math.ts`, `order.ts`, `geometry.ts`,
  `profile.ts`, `sweep.ts`, `gltf.ts`, `model-file.ts`) are Solid-free and
  GPU-free BY DESIGN so they can be checked headless (and, for the two
  model modules, run under bun in `tools/model.ts`); keep them that way.
  The rigs under `checks/`
  (`geometry-check`, `sweep-check`, `pick-check`, `order-check`,
  `gltf-check`) run on
  flux from the repo root: `bunx srt bundle -f --stdout
  packages/3d/checks/<name>.ts | target/release/flux -`. Run the ones
  touching what you changed.

## Components

| Component | Props |
| --- | --- |
| `Scene` | `width`, `height` (target pixels), `clearColor?`, `background?` (fragment GLSL), `samples?` (1/2/4/8 MSAA), `label?`, `ref?(scene)`, `output?(texture)`, `events?` (mesh pointer events, default on) |
| `Group` | `position?`, `rotation?` (Euler radians, XYZ order), `quaternion?` (either, not both), `scale?` (number = uniform), `visible?`, pointer events (below), `ref?(node)` |
| `Mesh` | `geometry`, `material`, transforms as Group, `params?` (per-mesh uniforms, merge semantics - no unset), pointer events (below), `ref?(mesh)` |
| `Sprite` | as Mesh minus `geometry`: a camera-facing unit quad, `scale` is its world size, rotation is ignored; pair with a `sprite()` material |
| `InstancedMesh` | as Mesh, plus `records` (interleaved per-instance floats; buffer capacity starts at the first value and grows on larger rewrites), `count?` (records drawn, default all), `bounds?` (local [minX..maxZ] over the population - without it the mesh never picks); the record buffer is component-owned and freed on unmount |
| `PerspectiveCamera` | `fov?` (vertical DEGREES, default 60), `near?`, `far?`, `position?`, `lookAt?`, `up?` |

Output composition: without `output`, `Scene` emits a minimal
`<texture width height>` leaf and nothing else is forwarded - anything
more goes through `output(texture)`, which renders in place of that leaf:
a `<d-texture>`, a leaf with blendMode/fit/pointer/layout props, or a
post-effect chain (`createShaderTarget` sampling the id with a
covering-triangle pass; created in the callback it disposes with the
Scene). Return null for no leaf at all and compose `scene.texture`
elsewhere. Called once, untracked, inside the scene context. Scene
`width`/`height` are target pixels and the leaf's own width/height are
layout, so render and display size separate - render at 2x and display
smaller for supersampling.

Camera control: `createOrbitCamera(scene, { target?, azimuth?, elevation?,
distance?, min/maxDistance?, min/maxElevation?, orbitSpeed?, rotateSpeed?,
zoomSpeed?, zoomAnchor?, rotateAnchor?, panSpeed?, viewport?, clampTarget? })`
- drag-to-rotate, pinch- and wheel-to-zoom, two-finger pan, optional
auto-orbit. Input runs on core's `createTransform` recognizer, so drag and
pinch arbitrate in the app-wide gesture arena (a viewport inside a scroller
does not double-handle) and rotation starts after the recognizer's slop;
`zoomSpeed` weights both wheel and pinch. Two-finger translation pans (the
scene tracks the fingers 1:1 at target depth, weighted by `panSpeed`) when
`viewport()` supplies `{ height, fov }` for the pixel-to-world mapping -
without it, it rotates like one finger; `clampTarget(target)` bounds where
a pan may put the pivot. Zoom aims
at the target unless `zoomAnchor(x, y, {eye, target})` maps the pinch focal
/ wheel cursor to a world point (ground hit, target-depth plane, ...) - then
that point stays pinned under the pointer and the target slides toward it;
only the app can build that mapping, since fov, aspect and element placement
are app state. Pair it with `rotateAnchor({eye, target})`: called at gesture
start, its point is projected onto the view axis and re-seats the pivot
without moving the picture, so a drag after an anchored zoom orbits what the
camera looks at, not wherever the zoom left the target. Spread
`orbit.handlers` onto the input-owning element, call `orbit.update(dt)`
from your onFrame (no frame loop of its own), and use its return - true
when the pose changed - to gate per-frame dependents like reprojecting
HUD overlays. `orbiting()` is reactive (HUD-safe); the pose is plain state via
`pose()`/`set()` (also the debug-command shape). It drives position and
target only; fov/near/far stay on scene.setCamera. In a component tree,
reach the scene via `<Scene ref>` or useScene().

Overlay projection: `scene.project(point)` maps a world point to scene
pixels (top-left origin, y down - the output texture's own space; `w` is
clip-space w, the camera-forward distance) and returns null for a point
at or behind the camera plane. It reflects a pending `setCamera`
immediately, so set-then-project in one tick is exact. `scene.viewProj(out?)`
copies the view-projection matrix for batch work. Never rebuild the
camera matrices by hand for a HUD.

Picking: `scene.pick(x, y)` is project()'s inverse - the camera ray
through a scene pixel, returning `Hit[]` (`{ mesh, distance, point }`,
world units, nearest first; every hit along the ray, not just the front
one). `scene.raycast(origin, direction)` is the world-space primitive
under it.
The index and the narrowphase live in the spatial core: every attached
mesh's local box is a leaf in a dynamic AABB tree the flush refits from
the fresh world matrices (O(moved) per frame, a query O(log meshes)), and
an ordinary mesh is then tested per triangle against its geometry's
shape (one CPU copy per distinct geometry, created with its GPU buffers),
so hits carry `face`, `uv` and a world-space `normal` facing the ray, and
a ray through a knot's hole misses. An instanced mesh is box-only (its
explicit population bounds; records are opaque), so its hits have none of
the three. Both methods
flush pending writes first (the lookAt/project immediacy contract), and
both skip invisible meshes.

Mesh pointer events - the element vocabulary one tree deeper:
`onPointerDown/Move/Up/Enter/Leave` as plain fields on any node (and as
Mesh/Group props). The nearest hit mesh is the target; down/move/up
bubble mesh -> ancestor groups (`stopPropagation()` stops the walk);
enter/leave fire on the mesh alone, pairing on hover changes. A
pointer-down CAPTURES its mesh until the up: moves and the up keep
dispatching to it off-mesh (the platform's captured-drag rule), with
`point`/`distance` null while the ray misses it. The event carries the
element fields (pointerId, pointerType, button, modifiers) plus `mesh`,
`currentTarget`, `point`, `distance`, and `x`/`y` in scene pixels.
Wiring: the built-in `<Scene>` leaf carries `scene.handlers`
automatically (opt out: `events={false}`); an `output` leaf or
imperative composition spreads `{...scene.handlers}` onto the element
showing the texture. `scene.handlers` assumes that leaf is LAID OUT at
the target size - true for the built-in leaf and a d-texture at natural
size, under any ancestor transforms or viewBox fits (the hit test
undoes them; localX/localY arrive in the leaf's layout frame). A leaf
laid out at a different size (the supersampling pattern) uses
`scene.handlersFor(() => ({ width, height }))` with its layout size.

Geometry generators take ONE options object, every field optional with
a default, named as Three names them: `box({ width, height, depth })`
(1x1x1); `plane({ width, height })`, `circle({ radius, segments })` and
`ring({ innerRadius, outerRadius, segments })` (XY, facing +z - rotate
`[-Math.PI/2, 0, 0]` for a floor); `sphere({ radius, widthSegments,
heightSegments })`; `cylinder({ radiusTop, radiusBottom, height,
radialSegments })` (y axis, capped; unequal radii taper it) and
`cone({ radius, height, radialSegments })`; `torus({ radius, tube,
radialSegments, tubularSegments })` (lying flat, hole on the y axis) and
`torusKnot({ radius, tube, tubularSegments, radialSegments, p, q })`
(standing y-up) - both oriented for the y-up world, unlike Three's z-up.
No positional form: `box()` is the default cube, `box({ label: "rock" })`
names it. Every options object (the profile kit's `extrude`/`lathe`/
`sweep`/`tube` too) also takes `label` and `layout` - `layout` makes the
generator emit that layout in one pass (standard channels written, the
extra slots zero), so `box({ layout: "colored" })` then
`fillColors(g, fill)` builds colored geometry without the
generate-then-repack copy; the result is byte-identical to
`withColors(box(), fill)`. `packGeometry(verts, indices, options?)` is
the tail every generator ends in, for your own generators.
`withAttribute(geometry, attr, fill, label?)` derives a copy of any
geometry (generator or hand-built) with one more channel after its
current layout; the source is untouched. `withColors(geometry, fill,
label?)` is the aColor vec4 case, keeping the "colored" preset name.
`fillAttribute(geometry, name, fill, first?, count?)` is the in-place
primitive under both: overwrites one channel the geometry's layout
already carries (withAttribute ADDS one), reading pos/normal/uv from the
buffer itself - so a builder that bakes transforms while writing hands
the baker world-space vertices. `fill` indexes relative to `first`.
`fillColors(geometry, fill, first?, count?)` is its aColor spelling.

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
node, a draw entry and a per-frame `uModel` write of its own. Both are
pure array math (Three's `applyMatrix4` + `mergeGeometries`), no GPU
call, and the source geometries are untouched. `geometryBounds(geometry)`
returns the cached local AABB `[minX, minY, minZ, maxX, maxY, maxZ]`, and
`rayBoxDistance(ox, oy, oz, dx, dy, dz, minX, .., maxZ)` is the picking
slab test (entry t >= 0 in units of the direction's length, 0 from
inside, -1 for a miss) - for ray-testing boxes you keep yourself
(triggers, collision volumes) without meshes you do not want to draw.

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
custom work along a path. `shape(profile, options?)` fills one flat (facing +z,
like circle); `triangulate(points)` is the ear-clipping core (fan
fallback, never drops a cap), exported for custom flat work. These pick
uint16/uint32 indices by vertex count automatically.

Materials:

- `unlit({ color?, map? })` - straight `[r, g, b, a?]` 0..1, premultiplied
  internally.
- `sprite({ color?, map?, transparent?, billboard? })` - unlit on a quad
  that turns to face the camera IN THE VERTEX STAGE (off the shared
  uCamRight/uCamUp, or uCamPos for `billboard: "fixed-y"`, which yaws
  only and stays upright on world y - Godot's BILLBOARD_FIXED_Y, the
  tree/character sprite; the default `"full"` is Three's Sprite, flat to
  the screen). No per-frame JS however many sprites. `transparent`
  defaults to TRUE here (cutouts; Three's SpriteMaterial default), cull is
  off. Draw with `createSprite(material)` / `<Sprite>`: a Mesh over a
  shared unit plane, no geometry argument, `scale` = world size, rotation
  ignored. Picks by a unit box around its center (its reach at any
  facing), so hits carry no normal/face/uv. `examples/sprites.tsx`.
- `shaderMaterial({ vertex, fragment, params?, textures?, depth?,
  depthWrite?, blend?, cull?, topology?, label? })` - your own GLSL, the
  custom-look escape hatch. The STANDARD UNIFORM SET: the vertex stage
  MUST declare and use `uniform mat4 uModel` (the mesh's world matrix,
  per entry) and `uniform mat4 uViewProj` (the camera, shared
  target-level params) - transform with
  `uViewProj * uModel * vec4(aPos, 1.0)`; a source missing either throws
  at shaderMaterial() creation. The rest is opt-in by declare-and-use:
  `uniform vec3 uCamPos` (the camera's world position, shared and written
  with uViewProj - the specular/fresnel view vector is
  `normalize(uCamPos - worldPos)`), `uniform vec3 uCamRight` / `uCamUp`
  (the camera's world-space view axes, shared likewise - a billboard is
  `center + uCamRight * x + uCamUp * y`; do NOT rebuild them from
  uViewProj rows, that carries the clip flip) and `uniform mat4 uNormal` (the world
  inverse-transpose, written beside uModel for this material's meshes;
  take `mat3(uNormal)` - correct under non-uniform scale, where
  mat3(uModel) bends normals off the surface). Attributes come from the
  geometry's layout by name; the ones the linked program actually reads
  (engine reflection, instance attributes excluded) must all be in the
  mesh's geometry layout or add() throws - so a used `in vec4 aColor`
  needs `withColors()` geometry and a custom channel needs
  `withAttribute()`. One program per class, one pipeline per layout met. Sources without `#version` get the standard
  pipeline preamble. App-driven uniforms beyond the standard set: seed
  via `params`, then write per mesh with
  `setMeshParams(mesh, { name: value })` (validated names; values persist
  across entry rebuilds; frame-rate-safe like setTransform) or declaratively
  with the `Mesh` `params` prop (same merge semantics - a key that
  disappears from the object keeps its old value; for per-frame values
  prefer `ref` + setMeshParams from onFrame, the setTransform split).
  Scene-wide values (a clock, a sun direction, fog) go through
  `scene.setParams({ uTime })` instead - one write for every mesh.
- `shaderMaterialClass({ vertex, fragment, ...pipeline state })` - the
  class/instance split for your own GLSL: compiles once, and
  `cls.instance({ params?, textures? })` returns a Material sharing that
  pipeline with its own values. `dispose()` lives on the class alone.
  `shaderMaterial(opts)` is exactly a class with one instance (its
  `dispose` forwards to the class).
- `instanceAttributes: [{ name, format }]` on either shader-material form
  makes an INSTANCED material: the vertex stage reads them as `in`
  variables beside the layout's own, and each drawn instance gets one
  record of the mesh's instance buffer. Its meshes come from
  `createInstancedMesh` (below); a `createMesh` mesh is rejected at add().

Instancing - one draw entry covering a population:
`createInstancedMesh(geometry, material, records, count?, { bounds?,
label? })` returns an ordinary Mesh whose entry draws the geometry once
per record. `records` is the interleaved per-instance data (stride = the
material's instanceAttributes summed, a mismatch throws), uploaded to a
mesh-owned buffer whose capacity starts at the records given. `count` picks how
many records draw (default all). Everything mesh works unchanged:
setTransform moves the whole population through one uModel, setVisible
zeroes the drawn count and restores the record count on unhide,
renderOrder/params/geometry/material swaps apply. `setInstances(mesh,
records, count?)` rewrites records from the start (count defaults to the
records written; more than capacity GROWS: capacity doubles into a
replacement buffer, the entry is re-pointed via `setDrawBuffers`, the old
buffer is freed), `setInstanceCount(mesh, n)` is the population dial (clamped to capacity;
frame-rate-safe), and `disposeInstances(mesh)` detaches and frees the
record buffer - the one explicit free, geometry-buffer rule. Records are
opaque data (position/yaw/tint/whatever your shader reads), NOT matrices:
a per-instance mat4 would be four vec4 columns reassembled in the shader,
but most fleets want a few floats. Picking: the library cannot know where
records place instances, so an instanced mesh has NO picking leaf unless
you pass `bounds` (local, covering the population) - then it picks and
transparent-sorts conservatively as one box. `examples/instanced.tsx` is
the live proof.

Background: `scene.setBackground(source | null)`, the `background` option
on createScene, and the reactive `Scene` prop. Fragment GLSL drawn as the
FIRST entry of the scene's own pass (attributeless fullscreen triangle,
depth off) - one target instead of a backdrop texture stacked under the
scene, with no separate resize plumbing. The source gets the
shader-target fragment contract exactly (vUV 0..1 top-left origin,
iResolution, fragColor; no `#version` line = the standard preamble), so
a `createShaderTexture` backdrop ports verbatim. Three's
`scene.background = color` is `clearColor` here; a texture-id form can
widen the signature later (a branded TextureId is a number, so
`string | TextureId` disambiguates at runtime). No app-driven uniforms:
a background is static art - anything animated is a mesh's own
shaderMaterial (or, until blend factors land, a separate shader texture
underneath, which translucent grounds also still need).

Lighting GLSL (`@solidrt/3d/glsl`): exported string constants composed
into shaderMaterial sources with plain template literals - `LIT_VERTEX`
(the standard vertex stage: clip position plus vWorldPos/vNormal/vUv
varyings, normals via mat3(uNormal)), `LIT_VERTEX_COLORED` (the same
plus the colored layout's aColor forwarded raw as vColor - using it makes
the material need that channel) and the pure functions `HEMISPHERE`
(`hemisphere(n, sky, ground)`), `LAMBERT` (`lambert(n, l)`),
`BLINN_SPECULAR` (`blinnSpecular(n, v, l, shininess)`), `FRESNEL`
(`fresnel(n, v, power)`). Lights, colors and exponents are arguments, so
nothing is pinned but the function names; `lit` is composed from these
same constants - customizing never means leaving the system.

Lights and `lit`: lights are graph NODES, like Three. `createDirectionalLight({
direction?, color?, intensity? })` / `<DirectionalLight>` is parallel light
travelling along `direction` in the node's LOCAL space (default `[0, -1,
0]`, a sun overhead; length ignored), so a parent Group's rotation turns it
and position/scale do not matter - deliberately a direction, not Three's
position-minus-target. `createHemisphereLight({ sky?, ground?, intensity?
})` / `<HemisphereLight>` is the ambient term, a gradient by the WORLD
normal's tilt (fixed to world up, the node's transform is ignored); one per
scene, the last attached wins. Placement goes through setTransform, the
light's own fields through `setLight(light, { ... })` (frame-rate-safe,
like setMeshParams). At most `MAX_LIGHTS` (4, exported from `/glsl`)
directional lights per scene - the fifth throws at add(); it is a
shader-source constant, fixed per app. `uLightDir` is core-driven: each
directional light's slot is a spatial-core shared-slot sink following
the node's world rotation, so a MOVING light costs no JS. The sync
rewrites the rest whenever a light attaches, detaches or changes a
field -
`uHemiSky`/`uHemiGround` (vec3, intensity folded in), `uLightCount` (int),
`uLightDir[MAX_LIGHTS]`/`uLightColor[MAX_LIGHTS]` (world-space vector
TOWARD the light, normalized; intensity folded into the color) - so a
custom fragment declaring those names reads the same list, and a light
change costs one write however many meshes. Everything starts black: a
lit scene with no light shows nothing, on purpose, like Three.

`lit(opts)` is the standard look beside `unlit`: hemisphere ambient plus
the directional list, Lambert diffuse, Blinn-Phong highlight when
`specular` (0..1 strength) is set with `shininess` (default 30), the
same `color`/`map`/`transparent` as unlit, `vertexColors: true` to
multiply by the colored layout's aColor (so the geometry must carry it),
and `triplanar: n` to sample `map` by world position at `n` repeats per
world unit, blended across the three axis planes by the normal. Triplanar
is an OPTION, not the default: generators emit 0..1 UVs per face, so a
map on a plane is a decal (UV) while a map on generated scenery wants one
density across parts of any size (triplanar); the map must be created
with `wrap: "repeat"`. Internally one `shaderMaterialClass` per option
combination (map x vertexColors x triplanar x transparent), cached for
the app's lifetime, one pipeline per vertex layout - a thousand lit
meshes share one program. The view vector comes from the shared uCamPos;
`uTriplanar` is declared only by the triplanar classes so the other
classes do not warn about an inactive uniform.

## Models

Authored models come in as glTF 2.0 (.gltf with its .bin and image files
next to it, or single-file .glb) and become a Group of meshes, Three's
`gltf.scene`. Three layers, use the lowest that fits:

- `parseGltf(bytes, resolve?)` - the pure parser (no engine, runs under
  bun and on flux): `ModelData` = `parts` (one per mesh node, its NAME
  kept, vertices in the standard layout with the node's WORLD transform
  baked in), `materials` (base color factor, `map` = index into `images`,
  `doubleSided`, `transparent` = alphaMode BLEND), `images` (the encoded
  PNG/JPEG bytes, undecoded) and `bounds`. A .gltf's external files come
  through `resolve(uri)` (uri as written, still percent-encoded;
  `gltfExternalUris(bytes)` lists them so an async caller can read them
  first); .glb and data: uris need none. Missing normals produce FLAT
  shading (the spec's rule): the primitive is un-indexed, one vertex per
  corner. A mirroring node flips the winding so `cull: "back"` still
  keeps the outside. Non-triangle primitives are skipped; a required
  extension the parser does not implement throws naming it, and Draco or
  meshopt compression throws "re-export without mesh compression" -
  Blender exports Draco by DEFAULT, so that is the first error a real
  file hits.
- `createModel(data, { material?, label? })` - uploads the images (repeat
  wrap, mipmapped), makes one material per glTF material (default `lit({
  color, map, transparent })`; pass `material(m, map)` for anything else,
  it is called once per material and shared), one mesh per part, all
  children of the returned `Model` (a Group): `add(scene.root, model)`,
  place it with `setTransform`, find parts by name in `model.parts`
  (`{ name, mesh }`), `model.bounds` for framing a camera. `dispose()`
  detaches it and frees the geometry buffers and textures - the model owns
  them, nothing else frees them.
- `loadGltf(path)` / `loadModel(path)` - read from `assets/` with flux:fs
  and build. `loadModel` reads the baked `.srtm` written by `srt tool
  3d/model <in.gltf|glb> -o assets/<name>.srtm`: the same parse run once
  under bun, stored in the GPU layout, so loading is views onto the file's
  bytes plus the image decodes. Numbers from a 32k-vertex, 6-texture model
  on a release client: `parseGltf` 124 ms on flux (22 ms under bun) against
  40 ms for the whole baked load - the runtime parse is fine for small
  models and a binary import (`import bytes from "./x.glb" with { type:
  "binary" }` then `createModel(parseGltf(bytes))`, see
  `examples/model.tsx`); bake anything big.

Not in the subset, reported or dropped: `doubleSided` is reported and NOT
applied (the standard materials cull back faces); vertex colors, tangents
and further UV sets are dropped; samplers are ignored (every texture
repeats); alphaMode MASK draws opaque; emissive/additive parts of a model
draw as their base color (a model's "glow" cards come out as dark wedges).
The follow-ups are filed in okf/backlog/3d-model-loader.md.

## Traps

- A model's vertices are in WORLD space at parse time (node transforms
  baked), so `model.bounds` and each part's geometry already include the
  file's placement; the Model group starts at identity and `setTransform`
  on it moves the whole thing. Parts cannot be moved relative to their
  glTF parent - that is the retained-hierarchy follow-up, not a bug.
- The y-down clip flip is baked into `perspective()`; scene code and
  geometry are plain y-up right-handed, and CCW-outward winding culls
  correctly with `cull: "back"`. Do NOT negate y anywhere else, and do not
  "fix" the negated row of `perspective()` - both would mirror the winding
  and show mesh interiors.
- `visible: false` keeps the entry, drawn with `instanceCount: 0` (a
  cheap off switch); unhiding writes 1, or the mesh's own record count
  when it is instanced - never a bare 1 into an instanced entry. Hidden
  meshes skip uModel writes; the fresh matrix is
  written on unhide. A freshly attached entry starts off the same way and
  the core's flush turns it on when it writes uModel - never add one
  live: it has no world matrix yet, and drawn before the sync microtask it
  flashes at the world origin for a frame.
- Instancing pairs strictly at add(), like layout: an instanced material
  needs a createInstancedMesh mesh (records included) and vice versa, and
  the record stride must match the material's attributes - each mismatch
  throws there. The instance buffer is MESH-owned (unlike shared geometry
  buffers): `disposeInstances` is its one free, and the mesh cannot be
  re-added afterwards. Capacity grows by REPLACEMENT, never resize:
  `setInstances` past capacity doubles (at least to the records written)
  into a new buffer and swaps it in - amortized like a dynamic array, same
  policy as @solidrt/2d; size the initial records to skip the copies.
- An instanced mesh without explicit `bounds` has no BVH leaf: it never
  picks, pointer events never target it, and its transparent sort key
  falls back to the node's world position. That is deliberate - records
  are opaque to the library, so any inferred box would be a guess. Supply
  `bounds` for anything pickable or transparent.
- Transparency is an EXPLICIT material flag, Three's rule: `unlit({ color:
  [r, g, b, 0.5] })` still draws opaque; `unlit({ ..., transparent: true })`
  (or `shaderMaterial({ transparent: true })`) builds the pipeline with
  `blend: "alpha"` and `depthWrite: false` (depth test stays on, so it hides
  behind opaques without occluding other translucents). The one inference:
  a `shaderMaterial` with any `blend` but "none" is transparent unless told
  `transparent: false` - every blended draw belongs after the opaques, and
  back-to-front is harmless for add/multiply. The scene owns the
  order: background, opaque meshes by `renderOrder` then add order,
  transparent meshes by `renderOrder` then back-to-front by the CENTER of
  the mesh's world bounds in view space (not the origin: off-origin geometry
  sorts by where it is; not the nearest bounds point: a big translucent
  ground plane would cover the small translucents on it) - one `setDrawOrder` from sync() whenever the list changed, a
  renderOrder changed, or (with two or more transparent meshes) the camera
  or a transparent mesh moved, and skipped when the resort lands on the
  permutation already issued. Per-mesh sort only: one non-convex translucent
  mesh still overlaps itself in vertex order, and two large interpenetrating
  translucents can sort wrong (center distance, not per-pixel) - that is the
  engine contract, no OIT. A `shaderMaterial({ transparent: true })`
  fragment must write PREMULTIPLIED output (`vec4(rgb * a, a)`).
- Rotation is stored as a QUATERNION (`node.quaternion`, `[x, y, z, w]`,
  always unit). There is exactly one rotation field: no `node.rotation`
  shadowing it, because a second field is a second thing to go stale (an
  aimed node whose Euler triple still reads as the old pose is the bug
  this model deletes). Euler triples are a boundary format only -
  `setTransform({ rotation })` and the `rotation` prop convert in,
  `getRotation(node, out?)` converts out.
- Euler triples are XYZ order (x applied first: `R = Rx * Ry * Rz`),
  Three's `Euler` default, so a triple copied from a Three scene means the
  same thing here. This CHANGED 2026-08-11: the old `compose()` built
  `Rz * Ry * Rx` (Three's `'ZYX'`) while its comment claimed XYZ. Every
  rotation triple then in the repo, examples, demos and projects was
  single-axis, which is order-independent, so the fix moved no pixels -
  verified, not assumed. There is ONE order and no order argument: a
  per-call order is how one triple ends up meaning two things.
- `getRotation` cannot recover the triple that was written, only a triple
  meaning the same rotation (and at the poles it pins z to 0 and folds the
  roll into x). It is for reading and debugging; anything composing or
  interpolating rotations works with the quaternion.
- `eulerFromQuat` extracts y with `atan2(m02, cos(y))`, NOT Three's
  `asin(m02)`: asin's derivative blows up at the poles, turning 1e-16 of
  matrix error into 1e-8 of angle. Same reason its pole branch starts at
  `cos(y) < 1e-7` rather than Three's `|m02| > 0.9999999` (which is
  `cos(y) ~ 4.5e-4` - three orders early, and inside that band Three
  silently discards real roll). Do not "restore parity" here.
- Aim with `lookAt(node, target, up?)`, never by extracting angles by
  hand. Three's `Object3D.lookAt` semantics deliberately: `target` and
  `up` are WORLD space (ancestor transforms are undone, and the ancestor
  chain is refreshed on the spot rather than waiting for the sync), and
  local +z ends up pointing at the target. To aim along a DIRECTION, add
  it to `worldPosition(node)` - the same conversion Three asks for.
  +z is the library's own sweep axis, so `extrude`/`sweep`/`tube` output
  needs no correction. For a y-axis solid (`cylinder`, `cone`) use
  `quatFromTo(q, [0, 1, 0], dir)` instead of correcting lookAt's +z.
  Divergences from Three, both deliberate: `up` is an argument, NOT a
  per-node field (Three's `object.up` is hidden state that costs a vector
  on every node), and degenerate frames pick a stable perpendicular
  instead of Three's epsilon nudge of the eye.
  There is no `setTransform(node, { matrix })`, and lookAt is a MUTATOR,
  not a rotation-returning function.
- `quatFromTo` is Three's `setFromUnitVectors`, renamed after Unity's
  `FromToRotation` / glam's `from_rotation_arc`: the Three name states a
  precondition instead of the operation, and ours has no such
  precondition (it normalizes). Check Unity/glam/Godot too before copying
  a Three name that reads as an artifact of its class layout.
- The composition set: `quatFromAxisAngle` (radians; normalizes the axis -
  Three/Unity/glam all require a unit axis and silently corrupt
  otherwise), `quatMultiply` (same order contract as the mat4 `multiply`:
  `a * b`, b applies first; does NOT renormalize - the unit product only
  drifts under long accumulation, and setTransform renormalizes on
  write), `quatSlerp` (shortest path across the double cover, constant
  angular velocity, unit output; the damped follow is
  `quatSlerp(q, q, target, 1 - Math.exp(-k * dt))`). All aim/verb usage
  live in `examples/aim.tsx`.
- `setTransform` NORMALIZES an incoming quaternion, and passing `rotation`
  and `quaternion` in one call throws. A non-unit quaternion scales
  geometry by `|q|^2` through `compose()` - Three leaves that trap open
  and documents it; we close it at the one write path instead of paying
  for a check in every compose.
- `lookAt` is exact for rotation and uniform scale up the chain. A
  non-uniformly scaled ancestor shears the frame, so the aim is
  approximate - Three has the identical limitation (both read the parent's
  upper 3x3 as if it were a rotation), and the fix is not to special-case
  it here but to not shear parents of things you aim.
- The package root's `lookAt` is the scene verb; `@solidrt/3d/math` keeps
  its own `lookAt` (the camera view matrix) on the SUBPATH ONLY, the same
  collision rule the Vec3 helpers follow - and the same Object3D/Matrix4
  split Three makes under one name. Do not re-export math's from the root.
- Transforms have ONE write path: `setTransform`/`lookAt`/`setVisible` (or
  the props that call them). Mutating `node.position` directly does not
  sync. Components have no `lookAt` prop - aim through a `ref`.
- A camera change is ONE `setTargetParams` write (uViewProj + uCamPos are
  target state), independent of mesh count - never reintroduce per-mesh
  camera writes (uEye-style per-mesh params are exactly the O(scene) cost
  the shared channel removed). Scene scale honestly: hundreds to a
  few thousand objects, bounded by the interpreter, not the GPU.
- SCENE-WIDE uniforms go through that same shared channel via
  `scene.setParams({ uTime })`, and this is the single highest-leverage
  pattern in the library. It merges an app-owned name in beside
  uViewProj/uCamPos/uCamRight/uCamUp - names merge, a target tolerates
  zero coverage, neither side clobbers the other. One write per frame
  however many meshes read it, with the motion itself in vertex shaders
  off that one clock. `params`/`setMeshParams` is the PER-MESH answer and
  is O(meshes) per frame; reach for it only when the value genuinely
  differs per mesh. (`scene.texture` IS the draw target id, so
  `setTargetParams(scene.texture, ...)` is the same write - setParams is
  the sanctioned spelling.)
- Vec3/Quat arguments are COPIED IN everywhere (`setTransform`, `lookAt`,
  `setCamera`, params), so ONE scratch array reused every frame is safe -
  allocating three arrays per node per frame is pure waste. The node's own
  `position`/`quaternion`/`scale` are the live arrays: read them, do not
  hand them out and do not mutate them (that write does not sync).
- `setTransform` early-outs on an unchanged value (rotation compared AFTER
  euler conversion), so driving every node unconditionally from `onFrame`
  costs only the compare for nodes that did not move. Compares are exact,
  like `setVisible`.
- Per-generator conventions - orientation, UV mapping, which axis a solid
  stands on, what a cap looks like - live on each generator's doc comment,
  not here. They are consistent (`plane`/`circle`/`ring` face +z, `torus`
  lies flat with the hole on y, discs and cylinder caps get a PLANAR disc
  map inscribed in the unit square) but the doc comment is the source.
- Entry rebuild order: `setGeometry`/`setMaterial` re-add the entry at the
  list END and dirty the order, so the next sync() re-sorts and the mesh
  keeps its place. `_transparent` on the mesh is the flag AS ATTACHED
  (setMaterial swaps `mesh.material` before the rebuild, so _detach must
  not read the new material's flag).
- `lathe` takes a CLOSED profile (a cross-section with thickness, or run
  to the axis at x = 0) - it is a solid of revolution, NOT Three's open
  polyline shell. An "open" outline must be closed by the author;
  otherwise the shape is simply wrong, there is no open-profile mode.
- `useScene()`/`Group`/`Mesh` throw outside `<Scene>` (default-less
  context).
- A `shaderMaterial` INSTANCE is the pipeline handle: identical sources
  compile twice - no dedupe by source value (deliberate; hidden
  content-keyed caches are the anti-pattern the GPU layer avoids). Create
  one per look at app scope, share across meshes, `dispose()` when done
  for good. Looks that differ only in params/textures are ONE
  `shaderMaterialClass` and many `instance()`s - the app-owned split, not
  a cache. A class instance has no `dispose` of its own; disposing the
  class invalidates every instance.
- A parameterised class whose variants (mapped/unmapped, ...) are SEPARATE
  classes may seed every variant with one param/texture object: a uniform
  a variant declares but does not use compiles out, and the engine then
  accepts the write with a warning and skips it. A name no variant
  DECLARES still throws at add().
- The standard-set contract is checked TEXTUALLY at shaderMaterial()
  creation (uModel and uViewProj must appear in the vertex source) and
  at add() for the per-entry names: a uModel or uNormal that is declared
  but never USED compiles out, and the scene's entry seed is then skipped
  with an engine warning (the engine rejects only names the program never
  declared). The shared names have no such backstop - a declared-but-unused
  uViewProj or uCamPos is skipped silently (shared params tolerate zero
  coverage), so the symptom is an untransformed or unlit render, not an
  error. Use what you declare.
- The layout scan is textual the same way: any `aColor` token in the
  vertex source - a comment counts - selects the "colored" layout, and
  the material then rejects standard geometry at add(). Do not mention
  aColor you do not read.
- Picking is the VOLUME tier: a hit means the ray crossed the mesh's
  transformed bounding box, not its surface. Never present `point` as a
  surface point (it is the box-entry point), and never add a
  triangle-accurate path in JS - per-triangle rays at mesh scale are
  interpreter-hostile; that tier is core work (BVH descent per the
  differentiators ladder).
- `scene.handlers` vs `handlersFor`: localX/localY arrive in the leaf's
  LAYOUT frame (every ancestor transform and viewBox fit is already
  undone by the element hit test). `handlers` therefore assumes leaf
  layout == target pixels; scaling by `getBoundingBox` would be WRONG -
  the box composes transforms, and it would double-correct the built-in
  leaf under a viewBox. Only a leaf whose layout size deliberately
  differs from the target (supersampling) needs `handlersFor`, fed the
  layout size the app itself set.
- Hover (enter/leave) reacts to pointer MOTION only: a mesh animating
  under a still pointer fires nothing until the next move - the same
  limit the element hit test has (hit-test-per-frame is an open platform
  item). Do not poll pick() per frame to fake it.
- Geometry local bounds cache on the Geometry (like its GPU buffers):
  geometry is immutable after creation. Mutating `vertices` after a mesh
  used them leaves stale bounds AND a stale GPU buffer - make a new
  Geometry instead.
- The background covers the whole target with depth off, drawn first: it
  REPLACES the clearColor visually (the clear still runs; you just never
  see it), and a `transparent: true` mesh blends over it in-pass since the
  background is always entry zero.
- The background pipeline/program are SCENE-OWNED (unlike shared
  material pipelines): setBackground(null), replacement, and dispose()
  destroy them. Do not hand the background's pipeline to anything else.
