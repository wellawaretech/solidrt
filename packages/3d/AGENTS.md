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
  `setVisible` - plain objects with dirty flags, batched to a microtask,
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
- One vertex layout everywhere: `aPos` vec3 + `aNormal` vec3 + `aUV` vec2.
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
| `Scene` | `width`, `height` (target pixels), `clearColor?`, `label?`, `ref?(scene)` |
| `Group` | `position?`, `rotation?` (Euler radians, x-y-z order), `scale?` (number = uniform), `visible?`, `ref?(node)` |
| `Mesh` | `geometry`, `material`, transforms as Group, `ref?(mesh)` |
| `PerspectiveCamera` | `fov?` (vertical DEGREES, default 60), `near?`, `far?`, `position?`, `lookAt?`, `up?` |

Camera control: `createOrbitCamera(scene, { target?, azimuth?, elevation?,
distance?, min/maxDistance?, min/maxElevation?, orbitSpeed?, rotateSpeed?,
zoomSpeed? })` - drag-to-rotate, wheel-to-zoom, optional auto-orbit. Spread
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

Geometry: `box(w?, h?, d?)`; `plane(w?, h?)`, `circle(radius?, seg?)` and
`ring(inner?, outer?, seg?)` (XY, facing +z - rotate `[-Math.PI/2, 0, 0]`
for a floor); `sphere(radius?, wSeg?, hSeg?)`;
`cylinder(rTop?, rBottom?, height?, radialSeg?)` (y axis, capped; unequal
radii taper it) and `cone(radius?, height?, radialSeg?)`;
`torus(radius?, tube?, radialSeg?, tubularSeg?)` (lying flat, hole on the
y axis) and `torusKnot(radius?, tube?, tubularSeg?, radialSeg?, p?, q?)`
(standing y-up) - both oriented for the y-up world, unlike Three's z-up.

Profile kit (2D outlines to solids, real texture UVs): a `Profile` is a
closed XY polygon, bare `[x, y]` points crease, `{ p, smooth }` points
share an averaged normal - `fillet(points, radius, segs?)` and
`roundRect(w?, h?, radius?, segs?)` emit those (arc corners smooth).
Winding is normalized, so either authoring direction works.
`extrude(profile, depth?, bevel?, bevelSegs?)` sweeps along z, centered,
with a quarter-round bevel at both rims; `lathe(profile, segs?, angle?,
start?)` revolves a CLOSED (x = radius, y = height) profile about the y
axis - watertight by construction, flat caps on partial sweeps;
`shape(profile)` fills one flat (facing +z, like circle);
`triangulate(points)` is the ear-clipping core (fan fallback, never drops
a cap), exported for custom flat work. These pick uint16/uint32 indices
by vertex count automatically.

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
  shared layout by name; sources without `#version` get the standard
  pipeline preamble. App-driven uniforms beyond the standard set: seed
  via `params`, then write per mesh with
  `setMeshParams(mesh, { name: value })` (validated names; values persist
  across entry rebuilds; frame-rate-safe like setTransform).

Lighting GLSL (`@solidrt/3d/glsl`): exported string constants composed
into shaderMaterial sources with plain template literals - `LIT_VERTEX`
(the standard vertex stage: clip position plus vWorldPos/vNormal/vUv
varyings, normals via mat3(uNormal)) and the pure functions `HEMISPHERE`
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
  written on unhide.
- Alpha does not blend in v1: pipelines are opaque (`blend: "none"`), a
  translucent color overwrites. Transparency waits on blend factors +
  sorting (research note, staging step 4).
- Rotation is Euler radians applied x, then y, then z. No quaternions in
  v1.
- Transforms have ONE write path: `setTransform`/`setVisible` (or the
  props that call them). Mutating `node.position` directly does not sync.
- A camera change is ONE `setTargetParams` write (uViewProj + uCamPos are
  target state), independent of mesh count - never reintroduce per-mesh
  camera writes (uEye-style per-mesh params are exactly the O(scene) cost
  the shared channel removed). Scene scale honestly: hundreds to a
  few thousand objects, bounded by the interpreter, not the GPU.
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
