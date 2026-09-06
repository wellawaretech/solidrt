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
- Node lifecycle: `add(parent, child)` attaches (re-parenting detaches
  first), `remove(child)` DETACHES an intact subtree - children stay
  under the removed node, core nodes free on leave and recreate on
  re-enter, so a removed subtree re-adds cleanly. Nothing is destroyed
  except instance record buffers (`disposeInstances`) and the scene
  itself. (@solidrt/2d's `removeGroup` DESTROYS its subtree instead - a
  sprite cannot exist outside its layer, so there remove means destroy.)
- Rendering is the runtime's. The target is `render: "auto"`: it
  re-renders when entries change, so a STATIC scene costs zero passes and
  the library registers no frame loop. Continuous animation is the app's
  own `onFrame` writing a signal (declarative) or `setTransform` on a
  `ref`-grabbed node (the frame-rate escape hatch - signals carry
  structure, per-frame motion goes straight to the scene).
- VIEWS: `scene.createView({ width, height, overrideMaterial?, depth?,
  clearColor?, ... })` renders the same scene into a second target from
  its own camera (`view.setCamera`, the scene's CameraUpdate shape). Each
  mesh gets one entry in the view's target bound as one more draw sink of
  its CORE node, so the one flush writes every target - the app writes
  nothing per view. Geometry buffers and (without an override) materials
  are shared; the light set and `scene.setParams` names fan out to every
  view, `view.setParams` is the view's own channel - and names a view
  sets itself (or its `fog` option, below) become VIEW-OWNED: the
  scene's setParams/setFog fan-out skips them from then on, so a view
  override survives scene-wide writes. A view's backdrop is its
  clearColor (the scene background draws on PROBES, not views); a view
  has no picking. LAYERS select what a target
  draws, Three's model exactly: `layers` on a mesh is its membership
  bitmask (default 1, `setLayers`/the `layers` prop, NOT inherited from
  Groups), and each target carries a mask (default 1) - `layers` on
  createScene/createView, live via `setLayers` on the scene handle and
  each view. A mesh draws where mask & layers is non-zero, so a minimap's
  marker meshes live on bit 2: invisible in the main render, drawn by
  the map view whose mask admits them. Shadow views follow the SCENE's
  mask (what the scene cannot see must not darken it), and
  pick()/raycast()/overlap()/sweep() skip scene-masked-out meshes like
  invisible ones - unless the query passes its own `{ layers }`, which is
  how a low-poly collision mesh lives undrawn in the scene yet answers
  ground and collision queries (the physics-collider pattern).
  Per-view fog: `fog: FogOptions | null` on createView overrides the
  scene's fog for that view (null = unfogged - the clear minimap over a
  fogged scene); absent follows the scene. `overrideMaterial` (Three's
  `scene.overrideMaterial`, scoped to the view) draws every mesh with one
  material - a depth pass, a normal/id visualizer - skips instanced
  meshes (unless the override itself declares their exact
  `instanceAttributes` record layout) and draws in add order. `depth: "texture"` exposes `view.depthTexture`, the shadow-map
  input; the same option on createScene exposes `scene.depthTexture`,
  the input for a depth-reading post effect in `output` (not combinable
  with `samples` - no multisampled sampleable depth). `ortho: { left, right, top, bottom }` on any camera swaps
  perspective for `orthographic()` (`fov` ignored; `ortho: null` returns);
  the scene's own camera takes it too, and pick() follows.
  `examples/scene-views.tsx` is the shape.
- CULLING is the core's, per target, on by default: every camera write
  (the scene's, a view's, a shadow tile's) also sets that target's
  frustum in the spatial core, and the flush switches an entry whose
  world box falls wholly outside it to instance count 0 - the same
  switch as `visible`, so a culled mesh costs nothing per frame and a
  still camera re-tests nothing (a camera move re-tests every sink on
  that target, in Rust; a node move re-tests its own). The box is the
  picking box (the local bounds through the world matrix, the
  Godot/Unity AABB test; Three uses spheres); a mesh without bounds
  (an instanced mesh with no explicit `bounds`) is never culled. Per
  mesh: `frustumCulled: false` (Three's name; `setCulling`) for geometry
  a vertex stage moves beyond its box - a fullscreen quad, a custom
  displacement - and `cullMargin` (world units, Godot's
  `extra_cull_margin`) for bounded displacement such as wind. Sprites
  cull by their quad's reach at any facing. A SKINNED part is culled by
  the union of its joints' boxes (the bake computes each joint's
  influence box in joint space, `ModelSkin.jointBounds`, .srtm VERSION
  5; the joint nodes carry them as culling-only bounds, outside the
  picking index), so the box follows the pose with no per-frame JS -
  Unity's bone bounds, Godot's per-bone AABBs; no `updateWhenOffscreen`
  knob is needed. Probe faces set no frustum (six cameras, one target).
  Shadow tiles cull casters against their light frustum, which is what
  makes `shadow.distance` and cascades cheaper.
- SHADOWS are a view: `<DirectionalLight castShadow shadow={{ mapSize?,
  bias?, normalBias?, radius?, camera? }}>` (`createDirectionalLight({ castShadow,
  shadow })`, `setLight`) makes the scene own an internal
  `createView({ depth: "texture", overrideMaterial: depth pass })` drawing
  the `castShadow` meshes (`<Mesh castShadow>`, `setCastShadow`) from an
  orthographic camera at the light's WORLD position along its world
  direction, `shadow.camera` (+-5, 0.5..500 by default) as the frustum.
  Any light may cast, bounded by the shadow-slot budget
  (MAX_SHADOW_MAPS = 8, its own constant: a directional light claims
  `shadow.cascades` consecutive slots, a point light six, a spot one,
  and a caster past the budget throws at attach). `<SpotLight castShadow
  shadow={{ mapSize?, bias?, normalBias?, near? }}>` is the same
  machinery with a PERSPECTIVE camera: at the light's world position
  along its world direction, fov = its cone (2 * angle), near from
  `shadow.near` (default 0.5), far from the light's `distance` (or the
  directional default 500 when 0) - one map, one slot, the same atlas
  and lookup. A perspective map's depth is nonlinear, so `normalBias`
  (world units) is the acne knob to reach for; `bias` acts in that
  nonlinear depth. `<PointLight castShadow>` casts in every direction
  with the same option set: six 90-degree face maps (world-axis
  aligned, slot order +X, -X, +Y, -Y, +Z, -Z) as six consecutive tiles
  of the same atlas, far from `distance` like a spot - so give a
  casting bulb a distance. No cube map: a receiver picks the face by
  the dominant axis of the light-to-point vector (SHADOW_LOOKUP), one
  projection, one hardware-compare tap - the Three/Godot/Unity-URP
  atlas route. `shadow.radius` (every casting light, default 1 = that
  one tap, a 2x2 bilinear compare) softens the edge: above 1 a 3x3 grid
  of hardware taps `radius` texels apart, Three's `shadow.radius` /
  Godot's `shadow_blur`, nine taps per receiver fragment. A first-person
  eye over a floor shows the texel stairs at any map size; 2 is the
  usual figure, past ~3 the taps separate into bands. Each face map renders a few degrees wider than its face
  (URP's fovBias) so a seam fragment's occluder is inside the map it
  samples - without the guard band every seam shows a lit slit - and
  PCF taps clamp at face-tile edges, so a face seam hardens slightly
  instead of bleeding into the neighbour.
  `shadow: { cascades: N }` (1..MAX_CASCADES = 4) replaces the box with
  N maps fitted to slices of the SCENE camera's frustum (near ..
  `shadow.distance`, default the camera far; the practical split; each
  slice's bounding sphere as an ortho box along the light, its centre
  snapped to the map's texel grid so edges do not swim; re-fitted
  whenever the scene camera or the light moves) - a receiver samples the
  tightest map that covers the point, fading into the next over the
  map's outer 10% (`SHADOW_BLEND`) so the hand-over is a band, not a
  seam; contact shadows stay sharp near the camera while the horizon
  still has coarse ones, and pulling `distance` in sharpens all of them.
  The box is the honest tier for a bounded scene; cascades are for a
  scene that outgrows it, at N times the shadow fill. Every map is a
  TILE of the scene's one shadow atlas (a `depth: "texture"` draw target,
  a grid of cells the largest `mapSize` wide, scaled down uniformly
  against `limits.maxTextureSize`), so N maps are ONE pass: the atlas
  depth binds as the target-level `uShadowAtlas` of the scene and every
  non-shadow view (a white texel when nothing casts); maps are MAP slots
  dealt in light order, a light's cascades consecutive and tightest
  first - `uShadowRect[j]` slot j's tile in atlas UV, `uShadowMatrix[j]`
  its view's own view-projection (the whole array is one write per
  shadow-camera move) - and per light i `uShadowFirst[i]`/`uShadowCount[i]`
  name its slots (count 0 = it does not cast) with
  `uShadowBias[i]`/`uShadowNormalBias[i]` its knobs; `SHADOW_SLOTS` in
  glsl declares the set. Every `lit` material RECEIVES by default
  (Godot's and Three's default); `lit({ receiveShadow: false })` opts a
  material out and drops the map from its program - a material option,
  as with vertexColors/triplanar, because the material picks the program
  (Godot's `disable_receive_shadows`). The factor is `SHADOW`'s one
  hardware-compare tap (sampler2DShadow, LEQUAL, the driver's 2x2 PCF)
  on each casting light's own term. `examples/shadows.tsx` (three
  casting lights) is the shape; `examples/cascades.tsx` the cascaded sun.
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
  (a `shaderMaterialClass` per option combination for unlit, lit and
  sprite alike: map x transparent x cull x alphaTest, lit's extras on
  top), `depth: true` + `cull: "back"` unless the material says otherwise
  (`cull: "none"` for double-sided geometry; lit flips the normal on back
  faces); an instance is just per-entry uniforms (`uColor`) and bindings
  (`uMap`).
- The pure pieces (`math.ts`, `order.ts`, `geometry.ts`,
  `profile.ts`, `sweep.ts`, `gltf.ts`, `model-file.ts`) are Solid-free and
  GPU-free BY DESIGN so they can be checked headless (and, for the two
  model modules, run under bun in `tools/model.ts`); keep them that way.
  The rigs under `checks/`
  (`geometry-check`, `sweep-check`, `pick-check`, `order-check`,
  `gltf-check`) run on
  flux from the repo root: `bunx srt bundle -f --stdout
  packages/3d/checks/<name>.ts | target/release/flux -`. Run the ones
  touching what you changed. `raycast-check.tsx` and
  `collision-check.tsx` are the exceptions: they assert the documented
  picking contract (triangle accuracy, the box tier, pick/raycast
  parity, layer masks, the `{ meshes }` filter) and collision contract
  (exact sweep times, the surface rule, the slide filter, layers and
  meshes on overlap/sweep, moveAndSlide's landing) against a real scene,
  so they run on the playback client instead:
  `bunx srt render packages/3d/checks/<name>.tsx --project --duration 3
  --size 128x128`. Run them whenever a doc edit touches picking or
  collision claims - two copies of this contract have drifted before.

## Components

| Component | Props |
| --- | --- |
| `Scene` | `width?`, `height?` (target pixels - both, or neither = FILL, below), `clearColor?`, `camera?` (partial CameraUpdate, `ortho` included - the declarative scene.setCamera; same state as `PerspectiveCamera`, use one form), `background?` (fragment GLSL, or a skybox `{ cube, intensity?, rotation? }`), `environment?` (`{ cube, intensity?, rotation? }`, the cube reflective materials mirror), `fog?` (`{ color, near, far }`, linear by camera distance), `toneMapping?` (`"none"` default or `"aces"`), `exposure?` (default 1), `layers?` (target mask, default 1), `depth?` (`"texture"` exposes scene.depthTexture; not with samples), `samples?` (1/2/4/8 MSAA), `label?`, `ref?(scene)`, `output?(texture)`, `events?` (mesh pointer events, default on) |
| `Group` | `position?`, `rotation?` (Euler radians, XYZ order), `quaternion?` (either, not both), `scale?` (number = uniform), `visible?`, pointer events (below), `ref?(node)` |
| `Mesh` | `geometry`, `material`, transforms as Group, `params?` (per-mesh uniforms, merge semantics - no unset), `renderOrder?`, `castShadow?`, `layers?` (membership bitmask, default 1), pointer events (below), `ref?(mesh)` |
| `Sprite` | as Mesh minus `geometry`: a camera-facing unit quad, `scale` is its world size, rotation is ignored; pair with a `sprite()` material |
| `InstancedMesh` | as Mesh, plus `records` (interleaved per-instance floats; buffer capacity starts at the first value and grows on larger rewrites), `count?` (records drawn, default all), `bounds?` (local [minX..maxZ] over the population - without it the mesh never picks); the record buffer is component-owned and freed on unmount |
| `PerspectiveCamera` | `fov?` (vertical DEGREES, default 60), `near?`, `far?`, `position?`, `lookAt?`, `up?` - or the Scene `camera` prop, the same state (last write wins) |
| `SpotLight` | transforms as Group, `direction?` (local aim, default [0, -1, 0]), `color?`, `intensity?`, `distance?` (falloff cutoff, 0 = none), `angle?` (cone half-angle DEGREES, default 60), `penumbra?` (0..1 rim fade, default 0), `decay?` (falloff exponent, default 2), `castShadow?`, `shadow?` (mapSize, bias, normalBias, near), `ref?(light)` |
| `PointLight` | transforms as Group (position is what matters), `color?`, `intensity?`, `distance?`, `decay?`, `castShadow?` (six face maps, six shadow slots), `shadow?` (mapSize, bias, normalBias, near), `ref?(light)` |

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

Fill (the default): omit `width`/`height` and the built-in leaf is laid
out at 100% of its parent's box (give it a sized parent, as on the web)
while the target follows the leaf's on-screen size in DEVICE pixels -
display scale, `designSize` fits and ancestor transforms included
(getBoundingBoxViewport x displayScale, applied from onLayout, so no
frame draws at a stale size). A bare `<Scene>` in a pane renders at
native density on any display, mesh events and `<OrbitCamera>` input are
wired automatically (event scaling reads the leaf's untransformed box
back with `getLayoutBox`), and the camera aspect follows the box. Fill
or fixed is decided at mount; `output` requires explicit sizes (the
target cannot follow a leaf it does not own), and giving exactly one of
width/height throws.

Fixed sizes still matter where the target is a measured quantity: probes
and checks that snapshot exact pixels, supersampling via `output`, or a
scene composited at a size unrelated to its layout. `width`/`height` are
DEVICE pixels and the leaf's layout is LOGICAL - a fixed 720-pixel scene
under a HiDPI `designSize` fit is stretched across ~1100 device pixels
and looks soft, and nothing warns; that trap is what fill removes (the
examples fill; scene-views and scene-post-effect keep fixed sizes to
show multi-view composition and supersampling). A custom `output` leaf
whose layout differs from the target
takes `handlersFor` (below), not `handlers`; `useScene()` works inside
`output` because it runs in the scene context. With an `<OrbitCamera>`
(or any SceneInput listener) in the scene, also spread
`{...useScene().input.handlersFor(layout)}` on the leaf - the mesh-event
and control-input channels are separate spreads with the same layout.

Camera control: `createOrbitCamera(scene, { target?, azimuth?, elevation?,
distance?, min/maxDistance?, min/maxElevation?, orbitSpeed?, rotateSpeed?,
zoomSpeed?, zoomAnchor?, rotateAnchor?, panSpeed?, viewport?, clampTarget? })`
- drag-to-rotate, pinch- and wheel-to-zoom, two-finger pan, optional
auto-orbit. The first argument is anything with the scene's `setCamera`: a
Scene, or a View to drive one view's camera independently (one orbit per
view, each handed the handlers of its own viewport element). Input runs on
core's `createTransform` recognizer, so drag and pinch arbitrate in the
app-wide gesture arena (a viewport inside a scroller
does not double-handle) and rotation starts after the recognizer's slop;
`zoomSpeed` weights both wheel and pinch. Two-finger translation pans (the
scene tracks the fingers 1:1 at target depth, weighted by `panSpeed`) when
`viewport()` supplies `{ height, fov }` for the pixel-to-world mapping -
without it, it rotates like one finger; `clampTarget(target)` bounds where
a pan may put the pivot. Zoom aims
at the target unless `zoomAnchor(x, y, {eye, target})` maps the pinch focal
/ wheel cursor to a world point (ground hit, target-depth plane, ...) - then
that point stays pinned under the pointer and the target slides toward it;
only the app can build that mapping, since fov and aspect are app state
(the point arrives in the input element's own pixels). Pair it with `rotateAnchor({eye, target})`: called at gesture
start, its point is projected onto the view axis and re-seats the pivot
without moving the picture, so a drag after an anchored zoom orbits what the
camera looks at, not wherever the zoom left the target. Spread
`orbit.handlers` onto the input-owning element, call `orbit.update(dt)`
from your onFrame (no frame loop of its own), and use its return - true
when the pose changed - to gate per-frame dependents like reprojecting
HUD overlays. `orbiting()` (the auto-orbit switch) and `active()` (the
frame-loop gate: orbiting with a non-zero rate - the predicate the 2d
camera and the first-person camera share) are reactive (HUD-safe); the
pose is plain state via `pose()`/`set()` (also the debug-command shape). It drives position and
target only; fov/near/far stay on scene.setCamera (or the Scene `camera`
prop).

In a component tree, skip the wiring: `<OrbitCamera azimuth={1.2}
distance={7} />` as a Scene child reaches the scene through context,
receives input from the scene's leaf (the built-in one automatically; a
custom `output` leaf spreads `{...useScene().input.handlersFor(layout)}`
beside its scene.handlersFor spread, same `layout`), defaults `viewport`
to the leaf's laid-out size plus the scene camera's fov, and pushes input
poses synchronously - no ref plumbing, no onFrame. Auto-orbit runs a
frame loop only while `active()`, so a paused camera keeps the app
demand-driven idle. The pose props are
initial values: runtime pose changes (and the debug-command hookup) go
through `ref`'s handle, whose set() also pushes the pose. Every other
prop is live - forwarded to the control as a getter and read where it
applies, never snapshotted - so clamps, rates, anchors and `viewport`
follow their props without a remount, and a clamp change re-clamps the
pose at once.

First-person control: `createFirstPersonCamera(scene, { position?, yaw?,
pitch?, min/maxPitch?, moveSpeed?, lookSpeed?, fly?, viewport?,
clampPosition? })` - a position plus yaw/pitch (yaw 0 faces -z, positive
turns left; pitch positive looks up), Unity's FirstPersonController shape
(look AND move in one control) where Three splits PointerLockControls
from a hand-written key loop. Look comes from pointer-move
movementX/movementY while `pointerLocked()`, from a one-finger drag
(arena-arbitrated through createTransform, viewport-relative with
`viewport`) while not, and from the right stick; move from WASD/arrows
(physical codes and logical keys both), the left stick, and Q/E for
down/up - bound always, inert unless `fly` is on. Walking (the default)
flattens the heading onto the ground plane at fixed height. Every option
but the initial pose is read where it applies (`fly` per update,
`clampPosition` per move, the rates and pitch clamps per input), so a
field changed on the options object takes effect on the next move: walk
and fly are one control. The control NEVER calls `lockPointer`
- click-to-lock and Escape-to-release are the app's window-level
decisions (see `examples/first-person.tsx`) - and has no collision of
its own: `clampPosition(next, current)` is the whole hook - bounds, a
floor height, or `moveAndSlide` over `next - current` against the
collision layer (see `examples/collision.tsx`). Spread `handlers` (pointer + key +
onBlur; keys reach only the FOCUSED node, so that element must hold
focus or be the window), call `update(dt)` from onFrame; `active()` is
reactive - a key held or a stick deflected - and gates the loop. Keys
cannot be polled (core has no key-state accessor), so held keys are
tracked from the down/up pair and `onBlur` drops them.

`<FirstPersonCamera>` as a Scene child wires all of it: with a
key-driven control registered, the built-in leaf becomes `focusable`
and takes focus on pointer down (the web canvas gesture), which routes
the keys to the control; a pointer-only scene (`<OrbitCamera>`) never
steals focus. A custom `output` leaf spreads
`useScene().input.handlersFor(layout, () => node)` and declares
`focusable` itself. The frame loop runs only while `active()`. Every prop
but the initial pose is live: `fly={flying()}` toggles walk/fly on the
running control (pose and held keys carry over, no remount, and
`clampPosition` may swap with it), `moveSpeed`/`lookSpeed` follow their
props, and a pitch clamp change re-clamps at once.

Overlay projection: `scene.project(point)` maps a world point to scene
pixels (top-left origin, y down - the output texture's own space; `w` is
the camera-forward distance in world units under either projection) and
returns null for a point at or behind a PERSPECTIVE camera's plane; an
ortho camera places every point (`w` may be <= 0 there - negative near
is legal ortho). It reflects a pending `setCamera` immediately, so
set-then-project in one tick is exact. `scene.unproject(x, y, w, out?)`
is its exact inverse: the world point at that pixel and camera-forward
distance `w` - project()'s `w` round-trips in both modes (the
drag-at-depth recipe: project the grabbed point once, keep its `w`,
unproject each move). `scene.viewProj(out?)` copies the view-projection
matrix for batch work. Never rebuild the camera matrices by hand for a
HUD.

Picking: `scene.pick(x, y)` is project()'s inverse - the camera ray
through a scene pixel, returning `Hit[]` (`{ mesh, distance, point }`,
world units, nearest first; every hit along the ray, not just the front
one). `scene.raycast(origin, direction, opts?)` is the world-space
primitive under it, and `scene.screenRay(x, y)` the ray itself
(`{ origin, direction }`; direction's camera-forward component is 1, so
`origin + w * direction` = unproject) for intersection work pick cannot
do - drag planes, ground grids, filtered raycasts. All three cast
exactly the same ray. raycast's `opts` filters the query:
`{ layers }` (Unity's layerMask) replaces the scene's mask for this ray,
and `{ meshes }` (Three's intersectObjects) is an include-list; a
per-frame ground query passes one or the other instead of skipping
skyboxes and actors by hand.
The index and the narrowphase live in the spatial core: every attached
mesh's local box is a leaf in a dynamic AABB tree the flush refits from
the fresh world matrices (O(moved) per frame, a query O(log meshes)), and
an ordinary mesh is then tested per triangle against its geometry's
shape (one CPU copy per distinct geometry, created with its GPU buffers),
so hits carry `face`, `uv` and a world-space `normal` facing the ray, and
a ray through a knot's hole misses. A large geometry's triangles are
BVH-indexed too - built by the first ray that reaches the shape, log-cost
after - so raycasting a merged static scene stays cheap (see the batching
advice). An instanced mesh is box-only (its
explicit population bounds; records are opaque): it is tested by the
box's twelve triangles, so its hits carry the struck face's `normal` and
no `face`/`uv` - and a ray from inside it meets the far side, the
surface contract overlap/sweep share. Both methods
flush pending writes first (the lookAt/project immediacy contract), and
both skip invisible meshes.

Collision: `scene.overlap(volume, opts?)` and `scene.sweep(volume,
motion, opts?)` are the same index's other two questions - what a volume
touches, and where a moving one first touches: Unity's
OverlapSphere/Box/Capsule and SphereCast/CapsuleCast/BoxCast, Godot's
intersect_shape and cast_motion (Three has the raycaster alone; every
Three game adds a physics library or three-mesh-bvh for this). A
`Volume` is a sphere (`{ center, radius }`), a capsule (`{ a, b,
radius }`, the radius swept along the segment - a character) or an
oriented box (`{ center, halfExtents, rotation? }`). overlap returns
`Overlap[]` - `{ mesh, point, normal, depth }`, the deepest contact per
mesh: the point on the mesh, the unit direction out of it and the depth
along it that clears the contact (Godot's get_rest_info, Unity's
ComputePenetration, per hit; unordered) - and sweep `Impact[]` -
`{ mesh, time, point, normal }`, per mesh its first touch with `time`
the fraction of the motion, earliest first. Both take raycast's `opts`
(`{ layers, meshes }`) and test per triangle in WORLD space against the
same shapes, so any transform holds, an instanced mesh or sprite counts
by its box, and the physics-collider pattern is one `{ layers }` away.
Two contracts to know: the tests are SURFACES (a volume wholly inside a
closed mesh with no triangle in reach touches nothing - the trimesh rule
in every engine), and a sweep from a volume already in contact reports
time 0 only while the motion closes in; leaving or sliding along the
contact is no hit, which is what lets a slide along a wall proceed. A
zero motion touches nothing.
`moveAndSlide(scene, volume, motion, opts?)` is the controller over
them: Godot's CharacterBody3D.move_and_slide and Unity's
CharacterController.Move as one PURE function - no node, no velocity;
the first-person camera composes it in `clampPosition`, a node-driven
body applies `result.motion` with setTransform. It pushes the body out
of anything it starts inside, sweeps, stops a skin short, slides the
rest along the contact plane up to `maxSlides` times, then snaps down
onto a floor within `floorSnap` unless the motion rises - and the
`floor` it reports is the one it ENDS on (within `floorSnap` below), not
one it touched on the way. `MoveOptions`
adds `up`, `floorMaxAngle` (45 degrees: flatter is floor, steeper a wall
the body slides down), `maxSlides` (6), `skin` (0.01) and `floorSnap`
(0.1, 0 off) to the query filters; the result is `{ motion, floor, wall,
ceiling, hits }`. Gravity is the caller's - fold it into `motion` and
zero it while `floor` is set (the snap keeps reporting the floor while
the body stands still) - and a walkable floor absorbs the vertical part,
so a body never creeps down a slope it can stand on.
`examples/collision.tsx` is the whole pattern: a capsule walker through
`clampPosition`, gravity as a frame loop that runs only while airborne,
the level on a collider layer bit beside its drawn one, pickups lit by
one overlap per move. Deliberately absent and additive when asked: a
step offset (Unity's stepOffset; Godot has none either) and a cylinder
volume (Godot only).

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
size, under any ancestor transforms or design-size fits (the hit test
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
node, a draw entry and a per-frame `uModel` write of its own. Merging
does not tax picking: a merged geometry's raycast narrowphase runs
through its triangle BVH (built on the first ray), so merge for draw
count without giving up ground queries. Both are
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

- `unlit({ color?, map?, transparent?, cull?, alphaTest?, fog? })` -
  straight `[r, g, b, a?]` 0..1 sRGB (decoded to linear light, see Color
  below), premultiplied internally; `cull` and
  `alphaTest` as on lit (a mapped cutout casts its cutout); `fog: false`
  opts out of the scene's fog (all four library materials take it).
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
  uViewProj rows, that carries the clip flip), `uniform mat4
  uInvViewProj` (the camera's inverse view-projection, shared likewise -
  a clip position back to world, the world-space ray through a pixel
  without knowing the projection), the output stage's `uniform float
  uExposure` / `uToneMapping` (compose `OUTPUT` from `/glsl` and end with
  `fragColor = outputColor(rgb, alpha)` to take the scene's exposure and
  tone mapping and encode like the library materials do; a fragment
  writing fragColor directly writes final encoded pixels) and `uniform
  mat4 uNormal` (the world
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
on createScene, and the reactive `Scene` prop. Drawn as the FIRST entry
of the scene's own pass (attributeless fullscreen triangle, depth off) -
one target instead of a backdrop texture stacked under the scene, with
no separate resize plumbing. Two forms:

- Fragment GLSL. The source gets the shader-target fragment contract
  (vUV 0..1 top-left origin, iResolution, fragColor; no `#version` line
  = the standard preamble), so a `createShaderTexture` backdrop ports
  verbatim, PLUS `in vec3 vRay`: the world-space view ray through the
  pixel, unnormalized (the vertex stage carries its clip position back
  through the shared uInvViewProj at the near and far planes). A
  directional sky - horizon gradient, sun disc, stars - is a few lines
  on `normalize(vRay)`. The background is an ordinary scene entry, so it
  may declare `uniform vec3 uCamPos` (the ray's origin) and any name
  written through `scene.setParams` (an app clock for an animated sky).
  Godot's sky shader and Unity's skybox material are the same idea; the
  radiance bake for environment lighting will consume this same source
  later, so a procedural sky written here lights the scene then. The
  preamble declares the OUTPUT set: end with `fragColor =
  outputColor(rgb, 1.0)` for a sky that takes the scene's exposure and
  tone mapping (the skybox form does); a direct fragColor write is final
  encoded pixels.
- A skybox `{ cube, intensity?, rotation? }` (SkyboxOptions): a cube
  map from createCubeTexture sampled along the same ray - Three's
  `scene.background = cubeTexture` with `backgroundIntensity` and
  `backgroundRotation`. `rotation` is a turn about world y in radians
  (the sky turns as a node with that rotation would); `intensity` a
  multiplier. Replacing a skybox with a skybox rewrites the entry's
  params and cube in place (no recompile), so the reactive prop can
  animate the rotation. Under an orthographic camera every pixel looks
  the same way, so a skybox is one flat color there. A 2D texture id
  throws at the samplerCube binding. `examples/skybox.tsx`.

The cube-map convention: a cube map holds what a GL lookup returns -
each face as seen from OUTSIDE the cube, GL's own (RenderMan) frame -
and every library lookup is a plain `texture(cube, dir)` in world space.
No shader flip, as in Godot and Unity, which convert images at import;
Three instead flips x in the shader for image cubes (`flipEnvMap`) and
not for rendered ones, so a ported Three shader drops its flip, and a
Three-style six-face image set (px, nx, py, ny, pz, nz as seen from
inside) is mirrored per image at load. Every bake here (the tool,
`equirectToCube`, the examples' JS skies) writes GL's table directly,
and a cube the scene renders itself needs nothing. Three's `scene.background = color` is `clearColor` here; a 2D
image form can widen the signature later (a branded TextureId is a
number, so the object form keeps it unambiguous). Translucent grounds
over a background still need blend factors (a separate shader texture
underneath until then).

Environment: `scene.setEnvironment({ cube, intensity?, rotation? } |
null)`, the `environment` option on createScene and the reactive `Scene`
prop - the cube map every `standard` material is lit by (always: the
split sum `envRadiance` at its roughness times PBR's `envBrdf` for the
specular, `envIrradiance` - the fully rough sample along the normal, as
Three's getIBLIrradiance and Godot read it - added to the hemisphere
for the diffuse) and every `lit({ reflectivity })` material mirrors,
typically the skybox's own cube turned with it. The cube to use is a
BAKED one: `bunx srt tool 3d/environment sky.hdr -o assets/sky.srte`
turns an equirectangular Radiance .hdr (Poly Haven's are CC0) into the
six faces plus the GGX-prefiltered mip chain in linear float, and
`await loadEnvironment("assets/sky.srte")` uploads it as an explicit
"rgba16f" chain (createCubeTexture's array-of-levels form: no generated
mipmaps, so no half-float render support needed - it works on every
device; created after an await, so not auto-freed). Unity convolves at
import the same way; Three's PMREMGenerator and Godot's radiance map do
it at runtime. The roughness-to-level rule is `ENVIRONMENT`'s:
roughness r samples level `r * (log2(size) - 2)`, so a roughness of 1
lands on the 4x4 level (ENV_ROUGH_FACE in environment-bake.ts), the
last one the bake convolves; a `mipmap: true` cube from six faces
(equirectToCube, a JS-baked sky) merely box-filters that chain, so its
rough reflections are sharper than they should be and its diffuse term
is a coarse average - fine for a sky gradient, wrong for a photograph.
A 128 environment (2 MiB, the default) lights any surface; for a
mirror-finish showpiece bake at 256, and for a crisp backdrop pair it
with a separate hi-res LDR skybox (a 2k panorama through equirectToCube)
as `background` while the .srte stays the `environment`. Scene-level like Three's
`scene.environment`, Unity's environment reflections and Godot's
sky-lit reflections: ONE `uEnv` samplerCube bound on every target the
scene draws into (a 1x1 black placeholder while unset) and one
shared-params write (`uEnvIntensity`, `uEnvRotation`, `uEnvOn`), however
many meshes reflect; no per-material envMap (Three's Basic/Phong
`envMap`) - a custom material composes ENVIRONMENT from
`@solidrt/3d/glsl`. `reflectivity` 0..1 is the
face-on weight, rising to 1 at grazing angles (Schlick), mixed in as
`rgb = mix(rgb, reflection, weight)`: 1 is chrome, ~0.05 a glossy
dielectric with rim reflections; Three's Phong `reflectivity` under its
MixOperation with a fresnel weight (Three's default MultiplyOperation
tints instead; not offered). The reflection blurs with `shininess`:
roughness `sqrt(2 / (shininess + 2))` picks a mip level of the cube
(`textureLod`), so the environment cube wants `mipmap: true`; a cube
without mipmaps stays sharp. `specularMap`'s red scales it like
`specular`. For `lit` it is not an ambient light source: the hemisphere
light stays its only ambient term (`standard` adds envIrradiance; SH9
is a later, additive mode). A declared `reflectivity` with no
environment set contributes nothing (uEnvOn 0), not a black reflection.
`examples/skybox.tsx` (a JS-baked sky), `examples/environment.tsx` (a
baked HDRI lighting the scene alone).

Reflection probes: `scene.createReflectionProbe({ position, size?,
near?, far?, layers?, clearColor?, label? })` renders the scene into a
cube map from a point - Three's CubeCamera, Unity's and Godot's
realtime ReflectionProbe - and returns `{ cube, setPosition, update(),
dispose() }`; `cube` is what `environment={{ cube }}` (a chrome ball
mirroring its surroundings) or `background` takes. A view under the
hood: one entry list mirrored from the scene, the light set and scene
params fanned out, its own layer mask (keep the mirroring object out of
its own probe with `layers`), the scene's background drawn first on
every face (the GLSL sky or skybox behind the meshes, through the face
camera, in linear light - what Three, Unity and Godot probes see), a
cube draw target (`createCubeDrawTarget` in core, rendered face by face
with `renderTarget(cube, face)`).
Nothing renders it but `probe.update()`: six scene passes, from the
meshes as the last frame's flush placed them, so call it when the
surroundings moved (every frame for a moving scene, once for a still
one), then the PREFILTER: the faces convolved on the GPU into a second,
`mipmap: true` cube target level by level (`renderTarget(chain, face,
level)`, one small pass each - 48 at 128 - the bake tool's GGX
importance sampling as a fragment, `createPrefilter` in environment.ts,
with the same roughness-to-level rule as a .srte chain), so `standard`
blurs a probe by roughness exactly like a baked environment; `prefilter:
false` skips it and hands out the sharp faces (Three's CubeCamera: a
mirror at every roughness). COST: the chain's passes are tiny, but a
pass that samples mip levels of a cube map carries a fixed GPU cost
(about 0.3 ms each on an Intel/Mesa laptop, the same at 8 samples or at
256 - measured 2026-09-05), so a prefiltered probe updated every frame
costs ~14 ms of GPU there against ~1 ms for the six face passes: fine at
60 fps on its own, the largest single item in a frame budget. Realtime
probes are the expensive option in every engine (Unity time-slices
them): update a prefiltered probe when the surroundings changed, or
every few frames, and keep `prefilter: false` for a probe that must
refresh every frame on a tight budget. The faces hold LINEAR light (the probe owns
`uOutputEncode` 0, `uToneMapping` 0 and `uExposure` 1 on its target,
names the scene's fan-out then skips), HALF FLOAT where the device
renders it (`limits.halfFloatRenderable`, every GLES 3 device here: a
sun's or an emissive's range survives into the reflection, as in every
engine's HDR probe) and 8-bit clamped elsewhere - the renderer decides
for all probes (Godot), not a per-probe knob (`probeFormat()` in
environment.ts) - and the probe never samples its own cube while
rendering (a black environment stands in: one bounce). The face cameras are plain world-up cameras through an
x-mirrored projection (`Camera.mirror`), because a GL cube face is seen
from outside; the engine inverts the front-face rule on cube target
passes so cull modes keep their meaning. `examples/probe.tsx`.

Baked sky: `scene.bakeBackground(size?)` is a reflection probe at the
origin that sees no mesh (layer mask 0): the scene's background - the
GLSL sky or the skybox - alone on its six faces, LINEAR (a sky ending in
`outputColor` bakes its light; one writing fragColor raw bakes those
bytes as light), prefilters it like a probe and returns the chain's
TextureId for `environment={{ cube }}`: Godot's sky-to-radiance bake, so
a procedural sky lights the scene with no light nodes, and the runtime
way to turn a hi-res LDR skybox into a properly convolved environment
(the `mipmap: true` box chain above is the sharper, cheaper
alternative). A snapshot at the probe format (half float where
renderable; default size 128): bake again when the sky changes, and
destroy the old cube - it is not auto-freed (an environment normally
lives as long as the app). `outputColor` leaves linear output UNCLAMPED
(the clamp sits in the display encode), so a sky's sun disc of 40.0
bakes as 40.0 and shows as the broad bright highlight on rough metal
that HDR is for. A sky that reads
`uCamPos` bakes from the origin; scene params (an app clock) are seen as
of the call. `examples/sky-lit.tsx`.

Panoramas: `equirectToCube(map, size, opts?)` converts an uploaded
equirectangular 2D texture (createImage, createTexture) into a cube
TextureId on the GPU, synchronously (six face passes straight into a
cube draw target of the panorama's format - rgba8, rgba8-srgb or
rgba16f, no readback; `opts` are createCubeTexture's - `mipmap: true`
for an environment). The center column faces -Z and the top row is +Y,
as in Godot's PanoramaSkyMaterial and Unity's Skybox/Panoramic; Three
centers +X, a quarter turn away. Leave the source texture's wrap at
clamp (`repeat` also wraps vertically and bleeds the poles). An HDR
panorama uploaded as rgba16f converts into a half-float cube (sharp:
a skybox, or `mipmap: true` for the box chain); its PREFILTERED form is
the bake tool above, whose CPU pipeline (`src/environment-bake.ts`:
decodeHdr, panoramaToCube, prefilterCube, the .srte encode/decode) is
pure TypeScript and bun-tested - there is no runtime .hdr decoder.

Fog: `scene.setFog(fog | null)`, the `fog` option on createScene and
the reactive `Scene` prop, in Three's two shapes: linear `{ color, near,
far }` (`Fog`; fades from near to far, fully fogged past far) or exp2
`{ color, density }` (`FogExp2`, Unity's default; `1 - exp(-(d *
density)^2)`, no start band, never quite opaque - 0.01 is ~63% at 100
units). Either form takes `height` + `heightFalloff` (Godot's fog
height, Unreal's height falloff): full fog at and below `height` (world
y, default 0), thinning by `exp(-(y - height) * heightFalloff)` above -
a valley fills, the hilltops and the sky stay clear; per fragment
height, not integrated along the ray, the cheap tier every engine ships
first. A fragment fades toward `color` by its RADIAL distance from
`uCamPos` (not view depth). It is ONE shared-params write (`uFogColor`,
`uFogNear`, `uFogInv` = 1/(far-near), `uFogDensity`, `uFogHeight`,
`uFogHeightFalloff`; the form not in use is 0, "no fog" is every rate
0, which the scene seeds at creation so there is no enable flag and no
branch - the shader takes the larger of the two distance factors times
the height term), fanned out to every view, so fogging costs nothing
per frame however many meshes. Every standard material (unlit,
lit, sprite) composes it after its alphaTest discard, mixed at the alpha
it writes (premultiplied stays premultiplied); `fog: false` on the
material drops the code from the program (Three's `material.fog`) - a
sky sphere, a far backdrop. A shaderMaterial opts in by composing `FOG`
from `/glsl` (declares the set; `fog(rgb, alpha, worldPos, camPos)`, or
`fogAdditive(rgb, worldPos, camPos)` for a `blend: "add"` look, which
fades toward black instead of the fog color).
The BACKGROUND is not fogged: it is entry zero with no depth or
distance, so match the fog color to `clearColor` or the background's
horizon, and put `far` at or inside the camera's far plane to hide the
clip. `examples/fog.tsx` cycles the forms over a valley;
`examples/cascades.tsx` fogs its field to the sky.

Color: the scene shades in LINEAR light and outputs sRGB, like Three
(ColorManagement), Godot and Unity's linear space - no gamma mode. Every
`[r, g, b]` color option is sRGB, what a color picker shows: material
`color` and `emissive`, light `color`, the hemisphere's `sky`/`ground`,
fog `color`; the library decodes it when it writes the uniform
(`srgbToLinear`/`linearColor` are exported for values you write straight
to a uniform yourself). Color MAPS decode through their format: create a
base color, emissive or sky image with `format: "rgba8-srgb"`
(createTexture, createCubeTexture; createModel does it for glTF's base
color and emissive images) - a plain rgba8 map reads as linear data and
renders washed out; data maps (normal, specular, roughness, light maps)
stay rgba8, and an HDR image is "rgba16f". Vertex colors are linear, as
glTF stores them. Every library fragment ends in the OUTPUT set's
`outputColor(rgb, alpha)`: exposure (`scene.setExposure`, default 1),
tone mapping (`scene.setToneMapping("none" | "aces")`, the reactive
`toneMapping`/`exposure` props) and the sRGB encode, premultiplied. The
scene target therefore holds encoded pixels like every texture the
runtime displays; `clearColor` is written as given and is NOT tone
mapped (with a curve on, draw the backdrop as a background), and a
transparent mesh blends in encoded space (Three's compromise; the
hardware-encode alternative would display wrong through the runtime's
raw sampling). What changed for a scene tuned before this: terminators
soften, mid-tones brighten, highlights widen - drop ambient rather than
lights. `emissiveIntensity` scales the emissive in linear light.

Lighting GLSL (`@solidrt/3d/glsl`): exported string constants composed
into shaderMaterial sources with plain template literals - `LIT_VERTEX`
(the standard vertex stage: clip position plus vWorldPos/vNormal/vUv
varyings, normals via mat3(uNormal)), `LIT_VERTEX_COLORED` (the same
plus the colored layout's aColor forwarded raw as vColor - using it makes
the material need that channel) and the pure functions `HEMISPHERE`
(`hemisphere(n, sky, ground)`), `LAMBERT` (`lambert(n, l)`),
`BLINN_SPECULAR` (`blinnSpecular(n, v, l, shininess)`), `FRESNEL`
(`fresnel(n, v, power)`), `PBR` (the GGX metalness/roughness model
`standard` shades with: `ggxSpecular(n, v, l, f0, roughness)` - one
light's lobe, to weight by `lambert(n, l)` and the light color like the
diffuse - `envBrdf(nv, roughness)` for the split sum's scale and bias on
f0, `DIELECTRIC_F0`, and the D/V/F pieces; it defines `PBR_PI`, not
`PI`), and the shadow trio composed IN ORDER:
`SHADOW_SLOTS` (the scene's shadow set: `uShadowAtlas`, per map slot
`uShadowRect[M]`/`uShadowMatrix[M]`, per light index
`uShadowFirst[N]`/`uShadowCount[N]` (its slots; a cascaded light has
several, tightest first), `uShadowBias[N]`, `uShadowNormalBias[N]`),
`SHADOW` (`shadowPoint(coord)` - clip to map point, `shadowInside(p)` -
does the map have it, `shadowSample(map, rect, p, bias)` - one tile's
factor as ONE hardware comparison tap (`uShadowAtlas` is a
sampler2DShadow; the engine binds the comparison sampler for that
declaration, so the 2x2 PCF weighting is the driver's), and
`shadow(map, rect, coord, bias)` composing the
three) and `SHADOW_LOOKUP`
(`lightShadow(i, worldPos, n)` - light i's factor, 1 when it does not
cast; it walks the light's slots and samples the first map that covers
the point, which is the cascade select, blended into the next map over
the outer `SHADOW_BLEND` of the map). A receiving fragment
multiplies light i's term by `lightShadow(i, ...)`, exactly what `lit`
composes; a non-receiving one composes none of the three and declares no
samplers. Lights, colors and exponents are arguments, so
nothing is pinned but the function names; `lit` is composed from these
same constants - customizing never means leaving the system.

A custom look is a citizen of the scene - lit by its lights, shadowed,
fogged, exposed and tone mapped like the stock materials - at one of
three tiers, top first:

1. STANDARD FRAGMENT, CUSTOM VERTEX. Any vertex stage that writes the lit
   varyings (vWorldPos, vNormal, vUv, plus vColor with `vertexColors`,
   vUv2 with `lightMap`) pairs with `litFragment(options)` /
   `standardFragment(options)` (`/glsl`): the exact fragment `lit` /
   `standard` compile, the same option names and defaults as their
   options with the texture options boolean. An instanced or displaced
   mesh keeps the stock shading whole (`examples/instanced.tsx`: the
   per-instance tint rides vColor). Instance it with the per-entry
   uniforms the source declares (uColor, uSpecular/uShininess or
   uMetalness/uRoughness, the maps opted into).
2. A SURFACE FUNCTION inside the stock fragment. `litFragment({ surface,
   prelude })`: `prelude` is file scope (uniforms and helpers; a uniform
   it declares is an ordinary `instance()` param), `surface` declares
   `void surface(inout Surface s)`, called once the program has filled
   the Surface struct from its options (base from uColor, the map and
   the vertex color; the normal, bent by the normal map; emissive,
   ambient, the light model's fields) and before it shades. Rewrite any
   field or `discard`; it reads the varyings, the declared uniforms and
   prelude's names, and runs in the shadow twin too, so what it discards
   casts no shadow. The struct is the contract, no local of the
   generated program is; colors are linear light, premultiplied
   throughout, `Surface.base` included. The material describes the
   surface, the package shades it (Godot's fragment(), Filament's
   material()).
3. A FRAGMENT OF YOUR OWN over the scene set. Compose `SCENE` (or
   `sceneSource({ lights, receiveShadow, env, fog })`, each flag leaving a
   declaration out): it declares uCamPos, uHemiSky/uHemiGround, the
   light list, the shadow set, the environment, fog and OUTPUT - declare
   none of them yourself. Build a `Surface` with `surfaceOf(base,
   normal)`, set the fields you mean, call `shadeBlinn(s, position)` or
   `shadePbr(s, position)` (premultiplied rgb back: hemisphere, every
   light with its shadow, the environment term, the emissive), add your
   own terms times the alpha, and end with `sceneOutput(rgb, alpha,
   position)` (fog, exposure, tone mapping, encode). A custom LIGHT MODEL
   loops `sceneLight(i, position, normal)` to `uLightCount` instead of a
   shade function: light i's direction and its color already attenuated,
   cone-faded and shadowed (zero when it cannot reach). The stock
   materials are built from this same set, so the tiers cannot drift;
   `probes/scene-set-probe.tsx` is the byte-identity rig that checks it.
   The demo `the-third-dimension.tsx` has tier 2 (the ground) and tier 3
   (the knot's rim term); `probes/spot-custom-material-probe.tsx` a
   sceneLight loop beside a lit() floor.
`standardFragment(options)` is the same for `standard`: lit's options
minus `specularMap`/`env` (the environment is always composed) plus
`metalnessMap`/`roughnessMap`, on the same `litVertex(options)`, with
`uMetalness`/`uRoughness` in place of `uSpecular`/`uShininess`.
`litShadowFragment(options)` is the depth-pass twin (same base, cutout
and surface function, nothing after them), so a discarding material
casts what it draws: build it on `litVertex(options)` with the OPPOSITE cull, instance
it with only the uniform values its source declares (per-entry params
reject unknown names), and pass it as the main instance's `shadow`.
It returns undefined when the options cannot discard - the scene's
default depth override is then already right, carry no `shadow`.
`UNLIT_VERTEX` / `unlitFragment` / `unlitShadowFragment` are the unlit
twins (no lighting flags, no cull; varyings vUv/vWorldPos only). TRAP:
a shadow program that never reads `n` (no triplanar, no surface function
using it) reflects `uNormal` inactive - set `normalMatrix: false` on that
instance or every caster move warns about the skipped write.

Lights and `lit`: lights are graph NODES, like Three. `createDirectionalLight({
direction?, color?, intensity? })` / `<DirectionalLight>` is parallel light
travelling along `direction` in the node's LOCAL space (default `[0, -1,
0]`, a sun overhead; length ignored), so a parent Group's rotation turns it
and position/scale do not matter - deliberately a direction, not Three's
position-minus-target. `createSpotLight({ direction?, color?, intensity?,
distance?, angle?, penumbra?, decay? })` / `<SpotLight>` is a cone from
the node's WORLD position along that same LOCAL `direction` (aim by
`direction` or a parent's rotation; place by setTransform): `angle` is
the cone half-angle in DEGREES ((0, 90], default 60 - degrees like
camera fov and like Unity/Godot; Three's radians convert as
`angle * 180 / PI`), `penumbra` the
0..1 fraction of it fading to the rim (default 0, a hard edge), and the
strength falls off as `1 / d^decay` (default 2) windowed to zero at
`distance` (0 = no cutoff) - Three's SpotLight semantics minus the
target object. `createPointLight({ color?, intensity?, distance?,
decay? })` / `<PointLight>` is the omnidirectional version: position
only, same falloff, no cone. `createHemisphereLight({ sky?, ground?, intensity?
})` / `<HemisphereLight>` is the ambient term, a gradient by the WORLD
normal's tilt (fixed to world up, the node's transform is ignored); one per
scene, the last attached wins. Placement goes through setTransform, the
light's own fields through `setLight(light, { ... })` (frame-rate-safe,
like setMeshParams). At most `MAX_LIGHTS` (8, exported from `/glsl`)
lights per scene, directional, spot and point together (the hemisphere
is not in the list) - the ninth throws at add(); it is a shader-source
constant, fixed per app. `uLightDir` and `uLightPos` are core-driven:
each light's slots are spatial-core shared-slot sinks following the
node's world rotation (direction, negated so the shader reads the
vector TOWARD the light) and world position, so a MOVING light costs no
JS. The sync rewrites the rest whenever a light attaches, detaches or
changes a field - `uHemiSky`/`uHemiGround` (vec3, intensity folded in),
`uLightCount` (int), `uLightType[N]` (LIGHT_DIRECTIONAL | LIGHT_SPOT |
LIGHT_POINT), `uLightDir[N]`/`uLightPos[N]`/`uLightColor[N]` (intensity
folded into the color) and `uLightParams[N]` (cosInner, cosOuter,
distance, decay) - so a custom fragment composing `LIGHT_SLOTS` +
`LIGHT_LOOKUP` from `/glsl` reads the same list through `lightVector(i,
worldPos, out l)` (returns the attenuation, 0 = skip the light; `lit`
is the shape), and a light change costs one write however many meshes.
A custom fragment that declares only the old directional subset
(`uLightCount`/`uLightDir`/`uLightColor`) still works - it just shades
every light as directional, so keep such materials to directional-only
scenes. Everything starts black: a lit scene with no light shows
nothing, on purpose, like Three. `examples/lamps.tsx` is the spot/point
shape (soft vs hard cone, casting spots, an orbiting bulb).

`lit(opts)` is the standard look beside `unlit`: hemisphere ambient plus
the directional list, Lambert diffuse, Blinn-Phong highlight when
`specular` (0..1 strength) is set with `shininess` (default 30), a
mirror of the scene's environment when `reflectivity` (0..1, the
face-on weight; see Environment above) is set, blurred by the same
`shininess`, the same `color`/`map`/`transparent` as unlit, `vertexColors: true` to
multiply by the colored layout's aColor (so the geometry must carry it),
`triplanar: n` to sample `map` by world position at `n` repeats per
world unit, blended across the three axis planes by the normal, and
`alphaTest: t` for a cutout (a fragment whose final alpha is below `t`
is discarded; Three's alphaTest, glTF MASK): opaque, depth-written, no
sorting, usually with `cull: "none"` for cards. Triplanar
is an OPTION, not the default: generators emit 0..1 UVs per face, so a
map on a plane is a decal (UV) while a map on generated scenery wants one
density across parts of any size (triplanar); the map must be created
with `wrap: "repeat"`. Any `map` on a surface seen at distance also wants
`mipmap: true` at creation, or it aliases as it recedes, and a tiled
surface seen at a grazing angle (a floor, a road) wants `anisotropy: 4`
or more beside it, or trilinear smears the far half into the mip its long
axis picked (`createModel` uploads its images with both; the device clamps
the level, `limits.maxAnisotropy` reports it).

The surface maps, each an option beside `map` and sampled at its uv:

- `normalMap` (+ `normalScale`, ONE float as in Unity/Godot - Three's
  Vector2 exists to flip DirectX-style green channels, and glTF mandates
  OpenGL-style +Y) bends the lit normal per texel. The tangent frame is
  built per fragment from screen-space derivatives (`NORMAL_MAP` in
  `/glsl`, Three's untangented path), so ANY UV-mapped geometry works
  with no tangent channel; the trade is mild seams on mirrored UVs. Not
  with `triplanar` (throws - triplanar samples by world position).
- `emissive: [r, g, b]` (sRGB like `color`) times `emissiveIntensity`
  (linear, default 1; glTF's emissive strength) and `emissiveMap` add
  light the lights do not provide, after the lighting terms,
  shadow-proof, fogged. `emissive` defaults to WHITE when `emissiveMap`
  is given - the map is the emission - fixing Three's gotcha where an
  emissiveMap alone shows nothing against the black default.
- `specularMap`: its RED channel scales `specular` per fragment (chrome
  and rubber on one mesh); with it `specular` defaults to 1.
- `lightMap` (+ `lightMapIntensity`) adds a baked-light texture by the
  geometry's aUV2 channel (`withAttribute(g, { name: "aUV2", format:
  "vec2" }, fill)`) - ADDED to the light sum like the hemisphere term, so
  a fully baked scene runs with no lights at all. Three's material-slot
  form; Unity and Godot bake at scene level, but here the material picks
  the program.
- `mapTransform: { offset?, repeat? }` samples every uv map of the
  material at `uv * repeat + offset` - ONE transform per MATERIAL
  (Godot's uv1_offset/uv1_scale, Unity's Tiling/Offset; deliberately not
  Three's per-texture transform, since a TextureId is a shared value
  whose sampling is creation-time state). aUV2 is exempt. Scroll it per
  frame with `setMeshParams(mesh, { uMapTransform: [ru, rv, ou, ov] })`.
  Also on `unlit`. Not with `triplanar` (throws - its repeat is the
  triplanar value). A cutout's shadow transforms the same way.

`examples/materials.tsx` shows all five.

`standard(opts)` is the metalness/roughness material, the look authored
assets expect (Three's MeshStandardMaterial, Godot's StandardMaterial3D,
Unity's Standard): every lit option but the Blinn-Phong knobs
(`specular`, `shininess`, `specularMap`, `reflectivity`), plus
`metalness` (0..1, default 0: a metal has no diffuse and reflects tinted
by `color`, a dielectric reflects 4% face-on), `roughness` (0..1
perceptual, default 1: 0 a mirror, 1 matte; one value widens the
highlight and blurs the environment alike; Unity's smoothness is its
inverse) and the packed data maps `metalnessMap` (its BLUE channel) and
`roughnessMap` (GREEN) - Three's two channel-select options over glTF's
ONE metallicRoughnessTexture, so pass the same texture to both; with a
map the factor defaults to 1 (the map is the value). Shading is GGX
(`PBR` in `/glsl`: distribution, height-correlated Smith visibility,
Schlick fresnel) per light in the same light and shadow loop, the
hemisphere on the diffuse, and the scene's environment ALWAYS - the
split sum, `envRadiance` at the roughness over the cube's mip chain
times the analytic `envBrdf` for the specular, `envIrradiance` on the
diffuse beside the hemisphere - with no `reflectivity` switch: the
environment is intrinsic to the model, and a baked environment with no
lights at all lights a scene (`examples/environment.tsx`). Light intensities read as lit's
(1 lights a white matte surface to 1; Godot's and Unity's convention -
a Three scene's intensities divide by pi). Without an environment a
metal shows only its highlights: no diffuse, nothing to reflect (Three
and Godot do the same), so give the scene one. `examples/standard.tsx`
is the sphere grid. Internally one
`shaderMaterialClass` per option combination (map x vertexColors x
triplanar x transparent x cull x alphaTest x fog x the surface maps),
cached for the app's lifetime, one pipeline per vertex layout - a
thousand lit meshes share one program, and the key's width costs nothing
by itself: classes are created lazily per combination USED, so the
program count is the app's distinct material configurations. The view
vector comes from the shared uCamPos; `uTriplanar` and `uAlphaTest` are
declared only by the classes that use them (the cutoff is a per-entry
value, so every alphaTest material shares one class) so the other
classes do not warn about an inactive uniform.

## Models

Authored models come in as glTF 2.0 (.gltf with its .bin and image files
next to it, or single-file .glb) and become a Group carrying the file's
node hierarchy, Three's `gltf.scene`. Three layers, use the lowest that
fits:

- `parseGltf(bytes, resolve?)` - the pure parser (no engine, runs under
  bun and on flux): `ModelData` = `nodes` (the retained hierarchy in
  pre-order - name, parent index, local TRS; matrix-form nodes are
  TRS-decomposed, shear dropped; nodes that carry no part, joint or
  animation target anywhere - cameras, lights, unused empties - are
  pruned), `parts` (one per mesh primitive, its node's NAME kept, `node`
  index, vertices in the standard layout LOCAL to the node - except
  skinned parts: "skinned" layout, model-space bind pose, `skin` index),
  `skins` (joint node indices + inverse binds), `clips` (the animations
  as baked channel buffers: node/path/interpolation, times, values),
  `materials` (base color factor, `map` =
  index into `images`, `doubleSided`, `transparent` = alphaMode BLEND,
  `alphaMode` as written and `alphaCutoff`, spec default 0.5, the
  normal and emissive slots, `metalness`/`roughness` factors and the
  packed `metalnessRoughnessMap` - standard's inputs), `images`
  (the encoded PNG/JPEG bytes, undecoded) and `bounds` (world-space rest
  pose, conservative for parts under rotated nodes). External
  files come through `resolve(uri)` (uri as written, still
  percent-encoded; `gltfExternalUris(bytes)` lists them so an async
  caller can read them first) - for a .gltf AND for a .glb, which is
  usually self-contained but may legally reference external images
  (real exporters do); data: uris need no resolver. Missing
  normals produce FLAT shading (the spec's rule): the primitive is
  un-indexed, one vertex per corner. A mirroring node chain (negative
  rest-pose world determinant) flips the part's index winding so
  `cull: "back"` still keeps the outside. Non-triangle primitives are
  skipped; a required extension the parser does not implement throws
  naming it, and Draco or meshopt compression throws "re-export without
  mesh compression" - Blender exports Draco by DEFAULT, so that is the
  first error a real file hits.
- `createModel(data, { material?, label? })` - uploads the images (repeat
  wrap, mipmapped, 4x anisotropic), makes one material per glTF material (default `standard`
  with the file's color, maps, normal scale, metalness/roughness and
  packed map, emissive and transparency - the glTF material model, so a
  scene showing a model wants an `environment` (a glTF metal in a scene
  with none renders near black); `material: (m, maps, skinned) =>
  lit({ color: m.color, map: maps.map ?? undefined, skinned })` for the
  Blinn-Phong look, or any other material; it is called once per material and shared), the node table as nested
  Groups with the file's local TRS, and one mesh per part under its node,
  all inside the returned `Model` (a Group): `add(scene.root, model)`,
  place it with `setTransform`, find parts by name in `model.parts`
  (`{ name, mesh }`), spin a wheel relative to its axle through
  `model.nodes` (`{ name, node }` in table order, parents first; names
  repeat when the file's do - `.find()` yours), `model.bounds` for
  framing a camera. Skinned parts get the `skinned: true` material
  variant and hang off the model ROOT (the spec ignores their node's
  transform; the palette places them - see the mixer below). `dispose()`
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

Loading is async everywhere but the binary import: loadGltf/loadModel
return promises, and the async value must be read the way Solid 2 async
works - inside a tracking scope whose result the JSX reads back, under a
`<Loading>` boundary. The worked shape is `examples/model-load.tsx`: the
component keeps the async read in a memo (`let loaded = createMemo(() =>
loadModel(path))`), derives everything - framing, mounting - in a second
memo that reads `loaded()` FIRST and returns the scene JSX, and returns
only that memo read; the window/view shell lives in the parent, above the
boundary. Reading the value in the component body instead throws
PENDING_ASYNC_UNTRACKED_READ, and any element the component builds before
the suspending read is orphaned on the boundary's retry and never freed
(the dev leak sentinel reports it) - so the suspending component creates
no elements of its own. Async here means the file read: the parse and
createModel run synchronously on main. Bake anything big to .srtm; when
a source glTF must be parsed at runtime, do the parse in an isolate
(parseGltf's result is plain data and copies across) and keep
createModel on main.

Placement: pieces of one authored set (a body and its fitted cosmetics)
export in one world space, so composing them is `add(group, model)` per
piece and nothing else - no placement math. SOCKETED items (a weapon in
a hand) are different: they bind to a joint, so they only land once a
skeleton exists (a rig-less export cannot place them at all). The joint
is an ordinary Group in `model.nodes` - find it by name and `add()` the
item under it; it then follows the pose, mixer-driven or hand-posed,
like any child transform. Two authoring cases (verified in
`probes/joint-cap-probe.tsx`): an item authored about its own socket
origin needs the plain `add()` and nothing more; one authored in the
RIG'S model space needs a socket Group between joint and item carrying
the joint's rest-pose inverse (at rest the item then sits exactly where
authored, posed it follows), since parenting stacks the joint's
transform on top of the authored placement. Skinned PARTS are the one
thing that never needs this: they hang off the model root and the
palette places them.

Applied: `doubleSided` (the default material draws it with `cull:
"none"`), alphaMode MASK (`alphaTest: alphaCutoff`), `normalTexture`
(+ scale; the derivative frame needs no tangents), `emissiveFactor` x
`emissiveTexture` with KHR_materials_emissive_strength folded into the
factor (a zero factor skips the map too - glTF's product rule, emission
off), `pbrMetallicRoughness` factors as `m.metalness`/`m.roughness` and
its packed texture as BOTH `maps.metalnessMap` and `maps.roughnessMap`
(standard's channel-select options; the default `lit` ignores them).
The `material(m, maps)` callback receives every uploaded texture
by lit()/standard() option name (`maps.map`/`maps.normalMap`/
`maps.emissiveMap`/`maps.metalnessMap`/`maps.roughnessMap`);
`data.materials` is in file order, so the calls arrive in file order.
A `.srtm` baked before the material records carried the PBR fields
(file version 3) is rejected by loadModel - re-bake with `srt tool
3d/model`.
Animation: `createMixer(model)` plays `model.clips` by name -
`mixer.play(name, { loop?, speed?, fadeMs? })` (fadeMs crossfades: the
named clip fades in, everything else fades out - Unity's CrossFade,
Godot's play-with-blend), `mixer.stop({ fadeMs? })`, `mixer.playing()`,
`mixer.onFinish` for `loop: false` clips (the pose holds at the end).
Playback is CORE-DRIVEN: there is no update() and no frame loop to
register - clips are registered with the spatial core once and players
sample, weight-blend and write joint TRS natively each frame, so a
playing character costs zero JS per frame (gate other work on
`mixer.playing()`). play() requires the model to be IN a scene (players
bind live arena nodes; removing the model drops them - a re-added model
plays again from play()). Three traps that follow from core ownership:
(1) players advance BEFORE your onFrame, which is therefore the
post-animation hook - read a freshly posed joint and overwrite it
(root-motion strips, skeleton copies) in plain setTransform, last write
wins, all published by the same flush; (2) the JS
`position`/`quaternion` fields of player-animated joints (and of the
model node under root motion "apply") go STALE (they hold the last JS
write) - read poses with `getTransform(node)`, which reads the core;
writes to such a node always go through (setTransform skips its
usual equal-value short-circuit there, so a teleport back to the last
JS-written spot is not lost); (3) a channel nothing plays leaves the node's pose
alone; (4) known gap: native pose writes bypass the scene's moved list,
so a TRANSPARENT mesh parented under a player-animated joint does not
re-trigger the back-to-front re-sort while it animates (opaque meshes,
palettes and picking are unaffected) - nudge the scene with any
setTransform if it shows, until the core-side transparent sort lands.
Root motion: `play(name, { inPlace? })` strips a clip's root travel
(Unity's applyRootMotion off, Godot's root_motion_track): the root's
x/z hold at the clip's first key and its height rebases onto the root's
rest position, the vertical bob intact. `createMixer(model, {
rootHeight })` rebases EVERY clip onto that height, pinned or not, for
an export whose clips ride above its rest pose. Unset, it is decided per clip by NET DRIFT of the
root's position track - last key minus first, past a fraction of the
model's height - so run cycles play in place and a taunt that roams and
returns does not (pinning THOSE pushes the slide into the feet);
`mixer.travels(name)` reads the verdict. The root is the topmost node
any position channel of the clip targets. The strip is baked into a
second core clip on first in-place play, so it costs no per-frame JS and
crossfades like any other clip. A GAME wants the travel kept and moved
onto the character: `createMixer(model, { rootMotion: "apply" |
"report" })` plays every clip fully pinned (all three root axes held)
and its yaw held too (the turn about +y stripped key by key, so the
lean and pitch of the pose survive), while the core samples the
authored root tracks at each player's time and hands the per-frame
delta on - a translation in the model's local frame and a yaw in
radians. "apply" adds both to the model node itself (Unity's
applyRootMotion: the character walks and turns where its clip says),
"report" accumulates them until `mixer.rootDelta()` takes them
(`{ position, yaw }`), for a controller to spend through its own
movement (Godot's get_root_motion_position). Zero per-frame JS in
"apply"; one read per frame in "report". Loop wraps are continuous
(the clip's net drift is added across the wrap), crossfades weight the
deltas like the poses, and a clip's travel is given in the root's
CURRENT facing (its own turn so far undone), so a turn that wanders
out and back ends where the clip says and two blending clips agree on
the frame. The object form `{ mode, up?, vertical? }` names the root's
parent-space up axis (default +y; the turn and height axis) and
`vertical: "pose"` keeps the height in the pose, delivering only the
horizontal travel (Unity's bake-into-pose Y - for a controller that
owns gravity). `inPlace` is ignored while rootMotion is set. Yaw is
the swing-twist about up (exact under any lean); a cubic rotation
track is linearized at 60 keys/s before its yaw is held. Verified on
Mixamo's standing turns (external/mixamo-turn). Skins need nothing further: each skin's uBones palette
(model-local jointWorld x inverseBind, sized to the RIG - an rgba32f
float texture, 4 texels wide, one row per joint, sampled in the vertex
stage via texelFetch, so there is no joint cap) is composed by the
spatial core at the frame's flush from the joint nodes themselves, in
any write order, and identical skins (a body/legs split, LODs) share one
computed-once texture. `sampleChannel` (pure, from the root) stays the
JS sampling core for checks and custom drivers;
okf/done/animation-core.md records the design.

Not in the subset, dropped: vertex colors, tangents and further UV sets;
morph targets (the "weights" channel path); samplers are ignored (every
texture repeats); additive blending draws as base color. The follow-ups
are filed in okf/backlog/3d-model-loader.md. The `.srtm` container is
VERSION 3 (node table in the header, node-local vertices, skins, clips);
older bakes are rejected - re-bake with `srt tool 3d/model`.

## Traps

- A model's vertices are LOCAL to their node; the file's placement lives
  in the node-table TRS, composed by the scene like any Group chain. So
  a part's `geometry` alone is at the origin, `model.bounds` (rest-pose
  world) is what frames a camera, and moving a named node moves its
  subtree. The winding flip for a mirroring node chain is baked into the
  index order from the REST pose - re-scaling a node across zero at
  runtime shows mesh interiors, so do not do that.
- Skinning is a VERTEX-STAGE effect: everything that runs off the
  retained tree sees the bind pose. A skinned mesh picks by its
  bind-pose triangles at the model root and its transparent sort key is
  the bind-pose box; moving the JOINTS never moves either, moving the
  MODEL moves both. Shadows are the exception: the shadow variants
  (depth and cutout) skin by the same uBones palette, so a caster casts
  its pose.
- A `standard` metal in a scene with no environment renders near black:
  its diffuse is zero and the black placeholder cube is all there is to
  reflect (Three and Godot render the same; Unity falls back to an
  ambient probe, this does not). glTF's default metallic factor is 1,
  so an untextured asset is all metal, and createModel's default is
  `standard`: give a model scene an `environment` (loadEnvironment's
  baked .srte, or the skybox's cube), or pass `lit` as the material.
- A reflection probe is a mirrored render (x-flipped projection, winding
  inverted by the engine): anything built from screen-space derivatives
  flips with it, so a normal-mapped surface shows its bumps INVERTED in
  a probe's reflection. Known and shared with every engine's mirrored
  views; a `uMirror` sign on the derivative frame is the fix when it
  matters.
- A generated cube chain (`mipmap: true` from six faces) is a box
  filter, not the GGX convolution the roughness-to-level rule assumes:
  rough reflections read too sharp and the diffuse `envIrradiance` is a
  4x4 average. Bake with `srt tool 3d/environment` for anything
  photographed; the JS sky gradients in the examples get away with it.
- Light intensities are the same numbers for `lit` and `standard`: 1
  lights a white matte surface to 1 face-on. A Three scene's intensities
  are a factor pi larger for the same look; divide when porting.
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
- Instanced casters: `castShadow` on an instanced mesh needs the class's
  `shadowVertex` (see shadows below); a class without one is skipped by
  shadow views, silently.
- An instanced mesh without explicit `bounds` has no BVH leaf: it never
  picks, pointer events never target it, and its transparent sort key
  falls back to the node's world position. That is deliberate - records
  are opaque to the library, so any inferred box would be a guess. Supply
  `bounds` for anything pickable or transparent.
- Transparency is an EXPLICIT material flag, Three's rule: `unlit({ color:
  [r, g, b, 0.5] })` still draws opaque, and opaque means it: the standard
  classes write alpha 1 when not `transparent` (the scene target is
  composited premultiplied, so a leaked texel or color alpha would punch
  a see-through hole in an opaque draw - the source of "white cutouts"
  on an alpha-mapped model drawn without alphaTest). A `shaderMaterial`
  writes its own fragColor: give an opaque look alpha 1 too.
  `unlit({ ..., transparent: true })`
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
  few thousand objects, bounded by the interpreter, not the GPU. A view
  is one more such write per camera change and one more entry per mesh
  at attach; a view's per-frame cost is the core's (one params write per
  sink per moved node), never JS.
- A CASTING light's position matters (nothing else about a directional
  light's position does): the shadow camera is placed AT the light node's
  world position, Three's rule, so a `castShadow` sun at the origin
  pointing down shadows nothing above it - give it a `position` above the
  scene and a frustum (`shadow.camera`) that covers the casters. Acne
  knobs are Three's: `shadow.bias` (map depth units) and
  `shadow.normalBias` (world units along the receiver normal, the one to
  reach for first, ~0.02); the depth pass culls FRONT faces (Three's
  shadowSide default), so closed casters need little bias but a
  back-culling plane casts only from its back. The shadow side follows
  the material's `cull` (Three's shadowSide rule, Godot's shadow pass):
  a `cull: "none"` foliage card or pane casts from both faces, and a
  UV-mapped `alphaTest` material casts its cutout (leaves, not
  rectangles), through the `Material.shadow` variant the standard
  classes carry (a `shaderMaterial` gets the cull side from its `cull`
  and supplies its own cutout variant as the `shadow` instance option).
  Opting out of receiving is on the
  MATERIAL here (`receiveShadow: false`), not the object (Three's
  `mesh.receiveShadow`) - Godot's split, and URP's. An instanced mesh
  casts when its class declares `shadowVertex` (the vertex stage reduced
  to position, instance placement included; the class builds one depth
  program from it plus the shared depth fragment, culling the shadow
  side, and every instance shares it) - without one the shadow views
  SKIP instanced meshes, since the plain depth override cannot know
  their records. `examples/instanced.tsx` casts. Every casting light
  is a full extra pass over the casters plus a sampler unit on every
  receiving program (MAX_LIGHTS of those are always bound, placeholders
  included), so cast from the lights that matter, not all of them.
- A mesh's entries are mirrored into every view at attach and dropped at
  detach; `setGeometry`/`setMaterial` rebuild them everywhere. An
  `overrideMaterial` is validated against every mesh's layout (at
  createView for the meshes present, at add() for later ones) exactly like
  a mesh's own material, so an override reading `aColor` throws for a
  standard-layout mesh. Views are disposed by the scene; `view.dispose()`
  only for dropping one early.
- SCENE-WIDE uniforms go through that same shared channel via
  `scene.setParams({ uTime })`, and this is the single highest-leverage
  pattern in the library. It merges an app-owned name in beside
  uViewProj/uInvViewProj/uCamPos/uCamRight/uCamUp and the scene's own
  fog, environment and output sets (uFog*, uEnv*, uExposure,
  uToneMapping) - names merge, a target tolerates
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
- Picking is triangle-accurate for ordinary meshes (`point` is a surface
  point, hits carry `face`/`uv`/`normal`) but box-only for instanced
  meshes: there `point` is where the ray meets the population `bounds`
  box, `normal` that face's, and `face`/`uv` are absent. Never present an
  instanced hit as a surface hit. Both tiers run in the spatial core (Rust); never
  add a per-triangle path in JS - rays at mesh scale are
  interpreter-hostile, and the core already does it.
- `scene.handlers` vs `handlersFor`: localX/localY arrive in the leaf's
  LAYOUT frame (every ancestor transform and design-size fit is already
  undone by the element hit test). `handlers` therefore assumes leaf
  layout == target pixels; scaling by `getBoundingBox` would be WRONG -
  the box composes transforms, and it would double-correct the built-in
  leaf under a design size. Only a leaf whose layout size deliberately
  differs from the target (supersampling) needs `handlersFor`, fed the
  layout size the app itself set.
- A custom fragment that ends in `fragColor = vec4(...)` bypasses the
  scene: no fog, no exposure or tone mapping, no encode - and a
  hand-rolled loop over `uLightDir` renders a spot light as a
  directional one (a lit rectangle on the floor, no cone). What you
  declare is what runs, and the engine injects nothing, so pick a tier
  (the three after the GLSL exports under "The model"): a stock fragment
  on your vertex stage,
  a `surface` function, or `SCENE` with `shadeBlinn`/`shadePbr` or
  `sceneLight` and `sceneOutput` at the end. Composing `FOG` and the
  `SHADOW_*` trio by hand still works and is no longer the shape.
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
- A color map created as plain rgba8 renders WASHED OUT: the fragment
  reads its encoded bytes as linear light and encodes them again. Create
  color images (base color, emissive, a sky's faces or panorama) with
  `format: "rgba8-srgb"`; keep data maps rgba8. A rendered rgba8 texture
  (a scene view, a shader target, a UI capture) holds encoded pixels and
  cannot be tagged, so as a `map` it needs `srgbToLinear` from SRGB in a
  custom fragment (no material option yet); a draw target you create
  yourself can be `format: "rgba8-srgb"` and then decodes on sample (it
  is sampler-only: no display, readback or copy).
- Reading the scene texture back (a probe's readTexture, a snapshot)
  gives ENCODED pixels: an expected linear value v shows as
  `linearToSrgb(v) * 255` - intensity 0.5 reads 188, not 128.
- The background pipeline/program are SCENE-OWNED (unlike shared
  material pipelines): setBackground(null), replacement, and dispose()
  destroy them. Do not hand the background's pipeline to anything else.
  A skybox is the same slot with the library's fragment; only a
  skybox-to-skybox replace keeps the entry (params and cube rewritten).
- The environment binds through the light rewrite's map set (uEnv
  beside uShadowAtlas on every receiving target, new views included) and
  directly on setEnvironment; the placeholder cube is app-lifetime like
  the shadow placeholder. A `lit` without `reflectivity` declares no
  environment sampler - the flag is part of the class key.
- overlap()/sweep() test SURFACES, the trimesh contract everywhere: a
  volume wholly inside a closed mesh with no triangle in reach touches
  nothing, and a body whose center has passed through a wall reports
  the push-out on the side its center is on. Keep a skin (moveAndSlide
  does) and never teleport a body into geometry expecting it to come
  out the far side.
- moveAndSlide owns no gravity and no velocity: fold the fall into
  `motion` every frame and zero it while `floor` is set. A walkable
  floor absorbs the vertical part (a body never creeps down a slope it
  can stand on), and with `floorSnap: 0` a standing body reports no
  floor unless its motion presses into one.
