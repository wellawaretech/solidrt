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
  one `setDrawParams` (the uMVP matrix) per changed mesh. The component
  face (`Scene`/`Group`/`Mesh`/`PerspectiveCamera`) syncs props into that
  core over context and renders nothing itself.
- Rendering is the runtime's. The target is `render: "auto"`: it
  re-renders when entries change, so a STATIC scene costs zero passes and
  the library registers no frame loop. Continuous animation is the app's
  own `onFrame` writing a signal (declarative) or `setTransform` on a
  `ref`-grabbed node (the frame-rate escape hatch - signals carry
  structure, per-frame motion goes straight to the scene).
- One vertex layout everywhere: `aPos` vec3 + `aNormal` vec3 + `aUV` vec2,
  uint16-indexed. Geometry GPU buffers are lazy, shared, and app-lifetime
  (owner-scoped free would break sharing); `disposeGeometry` frees them.
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
when the pose changed - to gate per-frame dependents like a `uCamPos`
write. `orbiting()` is reactive (HUD-safe); the pose is plain state via
`pose()`/`set()` (also the debug-command shape). It drives position and
target only; fov/near/far stay on scene.setCamera. In a component tree,
reach the scene via `<Scene ref>` or useScene().

Geometry: `box(w?, h?, d?)`, `plane(w?, h?)` (XY, faces +z - rotate
`[-Math.PI/2, 0, 0]` for a floor), `sphere(radius?, wSeg?, hSeg?)`,
`torusKnot(radius?, tube?, tubularSeg?, radialSeg?, p?, q?)` (standing
y-up, unlike Three's z-up).
Materials:

- `unlit({ color?, map? })` - straight `[r, g, b, a?]` 0..1, premultiplied
  internally.
- `shaderMaterial({ vertex, fragment, params?, textures?, depth?,
  depthWrite?, blend?, cull?, topology?, label? })` - your own GLSL, the
  custom-look escape hatch. The vertex stage MUST declare and use
  `uniform mat4 uMVP` (the scene writes projection * view * world into
  it); attributes come from the shared layout by name; sources without
  `#version` get the standard pipeline preamble. App-driven uniforms
  beyond uMVP: seed via `params`, then write per mesh with
  `setMeshParams(mesh, { name: value })` (validated names; values persist
  across entry rebuilds; frame-rate-safe like setTransform).

## Traps

- The y-down clip flip is baked into `perspective()`; scene code and
  geometry are plain y-up right-handed, and CCW-outward winding culls
  correctly with `cull: "back"`. Do NOT negate y anywhere else, and do not
  "fix" the negated row of `perspective()` - both would mirror the winding
  and show mesh interiors.
- `visible: false` keeps the entry, drawn with `instanceCount: 0` (a
  cheap off switch). Hidden meshes skip uMVP writes; the fresh matrix is
  written on unhide.
- Alpha does not blend in v1: pipelines are opaque (`blend: "none"`), a
  translucent color overwrites. Transparency waits on blend factors +
  sorting (research note, staging step 4).
- Rotation is Euler radians applied x, then y, then z. No quaternions in
  v1.
- Transforms have ONE write path: `setTransform`/`setVisible` (or the
  props that call them). Mutating `node.position` directly does not sync.
- A camera change rewrites every visible entry's uMVP (documented v1
  cost, fine at hundreds of meshes). Scene scale honestly: hundreds to a
  few thousand objects, bounded by the interpreter, not the GPU.
- Entry rebuild order: `setGeometry`/`setMaterial` re-add the entry at the
  list END. Irrelevant while everything is opaque + depth-tested; revisit
  when transparency lands.
- `useScene()`/`Group`/`Mesh` throw outside `<Scene>` (default-less
  context).
- A `shaderMaterial` INSTANCE is the pipeline handle: identical sources
  compile twice - no dedupe by source value (deliberate; hidden
  content-keyed caches are the anti-pattern the GPU layer avoids). Create
  one per look at app scope, share across meshes, `dispose()` when done
  for good.
- A shaderMaterial vertex stage without `uniform mat4 uMVP` (declared AND
  used) throws at mesh attach - the scene seeds uMVP on every entry and
  the engine rejects unknown uniform names.
