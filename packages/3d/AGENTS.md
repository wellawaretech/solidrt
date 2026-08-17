# @solidrt/3d - agent notes

A retained 3D scene graph above `@solidrt/core/gpu`. Meshes, materials and
a camera compile to ONE depth-buffered draw target (`createDrawTarget` +
one `addDraw` entry per mesh); the scene's output is an ordinary texture
id composited as a `<texture>` leaf, so it takes layout, transforms,
blendMode and pointer events like any element. Design rationale:
`okf/research/scene-graph-3d.md` in the repo.

## The model

- Two layers. The imperative core is Solid-free: `createScene`,
  `createMesh(geometry, material)`, `add`/`remove`, `setTransform`,
  `lookAt`, `getRotation`, `setVisible` - plain objects with dirty flags, batched to a
  microtask,
  one `setDrawParams` (uModel, plus uNormal for materials declaring it)
  per changed mesh and ONE `setTargetParams` (the shared uViewProj +
  uCamPos) per camera change, however many meshes. The component
  face (`Scene`/`Group`/`Mesh`/`PerspectiveCamera`) syncs props into that
  core over context and renders nothing itself.
- Rendering is the runtime's. The target is `render: "auto"`: it
  re-renders when entries change, so a STATIC scene costs zero passes and
  the library registers no frame loop. Continuous animation is the app's
  own `onFrame` writing a signal (declarative) or `setTransform` on a
  `ref`-grabbed node (the frame-rate escape hatch - signals carry
  structure, per-frame motion goes straight to the scene).
- Two named vertex layouts (`Geometry.layout`, absent = "standard"):
  "standard" is `aPos` vec3 + `aNormal` vec3 + `aUV` vec2 - what every
  generator emits - and "colored" appends `aColor` vec4, the per-vertex
  data channel (a tint, baked AO, any four scalars; standard name, your
  contents). Derive colored geometry with `withColors(geometry, fill)` -
  fill is a flat 4-per-vertex array or a per-vertex callback receiving
  `(index, pos, normal, uv)`. Geometry and material layouts must match
  (layout is stride); a mismatched pair throws at add(). The whole layout
  ships whether a material reads every attribute or not (inactive
  attributes only keep the stride), so colored vertices cost 12 floats
  regardless - keep data-light passes (a wireframe reading only aPos) on
  standard geometry.
  Indices are uint16 or uint32 - the `Geometry.indices` array type picks
  the draw's index format, so hand-built geometry past 64k vertices just
  uses a Uint32Array (generators emit uint16). Geometry GPU buffers are
  lazy, shared, and app-lifetime (owner-scoped free would break sharing);
  `disposeGeometry` frees them.
- Materials dedupe hard: one program + one pipeline per material CLASS
  (unlit color, unlit map), `depth: true` + `cull: "back"`; an instance is
  just per-entry uniforms (`uColor`) and bindings (`uMap`).

## Components

| Component | Props |
| --- | --- |
| `Scene` | `width`, `height` (target pixels), `clearColor?`, `background?` (fragment GLSL), `label?`, `ref?(scene)`, `output?(texture)`, `events?` (mesh pointer events, default on) |
| `Group` | `position?`, `rotation?` (Euler radians, XYZ order), `quaternion?` (either, not both), `scale?` (number = uniform), `visible?`, pointer events (below), `ref?(node)` |
| `Mesh` | `geometry`, `material`, transforms as Group, `params?` (per-mesh uniforms, merge semantics - no unset), pointer events (below), `ref?(mesh)` |
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
under it. The volume tier: hits test each mesh's local bounding box,
transformed exactly under any node transform (non-uniform scale
included), so results are conservative - a ray through a knot's hole
still hits (no `face`/`uv` fields until a triangle tier exists).
Broadphase is a dynamic AABB tree (BVH) the sync walk keeps current from
its own dirty set - maintenance is O(changed), a query O(log meshes) -
so per-pointer-move picking puts no ceiling on scene size. Both methods
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

Geometry: `box(w?, h?, d?)`; `plane(w?, h?)`, `circle(radius?, seg?)` and
`ring(inner?, outer?, seg?)` (XY, facing +z - rotate `[-Math.PI/2, 0, 0]`
for a floor); `sphere(radius?, wSeg?, hSeg?)`;
`cylinder(rTop?, rBottom?, height?, radialSeg?)` (y axis, capped; unequal
radii taper it) and `cone(radius?, height?, radialSeg?)`;
`torus(radius?, tube?, radialSeg?, tubularSeg?)` (lying flat, hole on the
y axis) and `torusKnot(radius?, tube?, tubularSeg?, radialSeg?, p?, q?)`
(standing y-up) - both oriented for the y-up world, unlike Three's z-up.
`withColors(geometry, fill, label?)` derives a "colored"-layout copy of
any standard-layout geometry (generator or hand-built), adding the
`aColor` vec4 channel; the source is untouched.
`fillColors(vertices, fill, first?, count?)` is the in-place primitive
under it: writes the aColor slots of a colored-layout interleave you
already own (a merging builder's packed buffer), reading pos/normal/uv
from the buffer itself - so a packer that bakes transforms while writing
hands the baker world-space vertices. `fill` indexes relative to
`first`. It trusts the buffer's layout (no tag to check); withColors is
the checked path.

Profile kit (2D outlines to solids, real texture UVs): a `Profile` is a
closed XY polygon, bare `[x, y]` points crease, `{ p, smooth }` points
share an averaged normal - `fillet(points, radius, segs?)` and
`roundRect(w?, h?, radius?, segs?)` emit those (arc corners smooth).
Winding is normalized, so either authoring direction works.
`extrude(profile, depth?, bevel?, bevelSegs?)` sweeps along z, centered,
with a quarter-round bevel at both rims; `lathe(profile, segs?, angle?,
start?)` revolves a CLOSED (x = radius, y = height) profile about the y
axis - watertight by construction, flat caps on partial sweeps;
`sweep(profile, path)` runs the profile along an open 3D polyline with
MITRED joints (each cross-section sits on its bend's bisector plane, so
bends never gape or overlap) and flat caps at both ends. The path
mirrors the profile convention: bare `[x, y, z]` points crease (a strap
folding over an edge), `{ p, smooth }` points shade continuous (tag a
sampled curve's points); the profile's y starts as close to world up as
the first segment allows, then parallel-transports without spinning.
Closed loops are NOT supported yet - overlap the ends by a segment to
fake one. `tube(path, radius?, radialSegs?)` is the round-profile
shorthand (wire, rope, pipe), and `pathFrames(path)` exports the
per-segment frames (tangents, cross-section axes, arc lengths) for
custom work along a path. `shape(profile)` fills one flat (facing +z,
like circle); `triangulate(points)` is the ear-clipping core (fan
fallback, never drops a cap), exported for custom flat work. These pick
uint16/uint32 indices by vertex count automatically.

Materials:

- `unlit({ color?, map? })` - straight `[r, g, b, a?]` 0..1, premultiplied
  internally.
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
  `normalize(uCamPos - worldPos)`) and `uniform mat4 uNormal` (the world
  inverse-transpose, written beside uModel for this material's meshes;
  take `mat3(uNormal)` - correct under non-uniform scale, where
  mat3(uModel) bends normals off the surface). Attributes come from the
  geometry's layout by name; a vertex stage reading `in vec4 aColor` opts
  the material into the "colored" layout, and its meshes then need
  `withColors()` geometry. Sources without `#version` get the standard
  pipeline preamble. App-driven uniforms beyond the standard set: seed
  via `params`, then write per mesh with
  `setMeshParams(mesh, { name: value })` (validated names; values persist
  across entry rebuilds; frame-rate-safe like setTransform) or declaratively
  with the `Mesh` `params` prop (same merge semantics - a key that
  disappears from the object keeps its old value; for per-frame values
  prefer `ref` + setMeshParams from onFrame, the setTransform split).

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
plus the colored layout's aColor forwarded raw as vColor - using it opts
the material into that layout) and the pure functions `HEMISPHERE`
(`hemisphere(n, sky, ground)`), `LAMBERT` (`lambert(n, l)`),
`BLINN_SPECULAR` (`blinnSpecular(n, v, l, shininess)`), `FRESNEL`
(`fresnel(n, v, power)`). Lights, colors and exponents are arguments, so
nothing is pinned but the function names; future lit material classes
compose from these same constants - customizing never means leaving the
system.

## Traps

- The y-down clip flip is baked into `perspective()`; scene code and
  geometry are plain y-up right-handed, and CCW-outward winding culls
  correctly with `cull: "back"`. Do NOT negate y anywhere else, and do not
  "fix" the negated row of `perspective()` - both would mirror the winding
  and show mesh interiors.
- `visible: false` keeps the entry, drawn with `instanceCount: 0` (a
  cheap off switch). Hidden meshes skip uModel writes; the fresh matrix is
  written on unhide. A freshly attached entry starts off the same way and
  sync() turns it on when it writes uModel - never add one live: it has no
  world matrix yet, and drawn before the sync microtask it flashes at the
  world origin for a frame.
- Alpha does not blend in v1: pipelines are opaque (`blend: "none"`), a
  translucent color overwrites. Transparency waits on blend factors +
  sorting (research note, staging step 4).
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
- SCENE-WIDE uniforms go through that same shared channel, and this is the
  single highest-leverage pattern in the library. `scene.texture` IS the
  draw target id, so `setTargetParams(scene.texture, { uTime })` merges an
  app-owned name in beside uViewProj/uCamPos - names merge, a target
  tolerates zero coverage, neither side clobbers the other. One write per
  frame however many meshes read it, with the motion itself in vertex
  shaders off that one clock. `params`/`setMeshParams` is the PER-MESH
  answer and is O(meshes) per frame; reach for it only when the value
  genuinely differs per mesh. (A `scene.setParams` wrapper is backlogged;
  the channel underneath it is what matters and it is here today.)
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
  list END. Irrelevant while everything is opaque + depth-tested; revisit
  when transparency lands.
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
  for good.
- The standard-set contract is checked TEXTUALLY at shaderMaterial()
  creation (uModel and uViewProj must appear in the vertex source) and
  strictly at add() for the per-entry names: a uModel or uNormal that is
  declared but never USED compiles out, and the scene's entry seed then
  throws at attach (the engine rejects unknown entry uniform names). The
  shared names have no such backstop - a declared-but-unused uViewProj or
  uCamPos is skipped silently (shared params tolerate zero coverage), so
  the symptom is an untransformed or unlit render, not an error. Use what
  you declare.
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
  see it), and a translucent mesh does not blend over it in-pass yet: the
  engine has blend "alpha" (order-dependent), but the library has no
  transparent sort or renderOrder, so materials still draw opaque - the
  fade-over-backdrop look needs the two-layer composition until then.
- The background pipeline/program are SCENE-OWNED (unlike shared
  material pipelines): setBackground(null), replacement, and dispose()
  destroy them. Do not hand the background's pipeline to anything else.
