# @solidrt/3d examples

One concept per file; run with `bunx srt run <file>` from an app that
depends on `@solidrt/3d` (or in-repo from the package directory).

- `scene-basic.tsx` - the whole v1 surface: a `<Scene>` composited as a
  texture leaf, `<PerspectiveCamera>`, a ground plane, a spinning
  `<Group>` of unlit meshes with real depth-buffer occlusion, geometry
  and pipeline sharing, and the one-signal onFrame drive.
- `sweep-paths.tsx` - swept solids along polylines: a flat strap folding
  over a crate (bare path points crease on the mitred bends) and a coiled
  tube (smooth-tagged helix, one continuous mesh), lit so the
  creased-vs-smooth normals actually show.
- `polyhedra.tsx` - the polyhedron family: the four platonic solids
  flat-shaded (`detail: 0` keeps face normals), the icosahedron at
  `detail` 1, 2 and 3 turning into the icosphere, and a wireframe pair
  showing why - `sphere()` bunches slivers at its poles at the same
  triangle count where the icosphere's triangles stay uniform.
- `lit.tsx` - the lit material and light nodes: `<HemisphereLight>` plus
  a warm key `<DirectionalLight>` turning inside a spinning `<Group>` and
  a fixed cool fill,
  a glossy sphere (specular/shininess), a transparent glass sphere, and
  one checker map shared by a UV-mapped cube (stretched per face) and
  two triplanar meshes tiling it at one world density.
- `aim.tsx` - the rotation verbs, one pointer each tracking an orbiting
  target: `lookAt` for a +z solid, `quatFromTo` for aiming a y-axis cone,
  and a `quatSlerp` damped follow that visibly lags; all driven from
  onFrame through refs, no per-frame signals.
- `exits.tsx` - lifecycle animation on nodes: crates that pop in
  (`from`) and shrink away (`exit`, an ease-in of its own) when tapped,
  the component unmount being the `destroy` the exit rides on, a ghost
  while it leaves; a tap on empty space toggles the whole shelf through
  `<Show>`, and the Group's `stagger` cascades its crates out and back in.
- `pick.tsx` - mesh pointer events under an `<OrbitCamera>`, the scene
  as the root of the walk: hover tints (enter/leave), a tap pops, a drag
  on the crate slides it over the floor without orbiting (its down claims
  the press), a drag elsewhere orbits and a wheel zooms through the feed
  listening at the root, the ring claims the wheel to spin, a tap on
  nothing un-pops (the scene's own `onTap` with `mesh` null), and a
  Group hears its children's downs through bubbling; a STATIC scene
  rendered only when an event changes something.
- `scene-background.tsx` - a fragment-GLSL background drawn inside the
  scene's own pass (`<Scene background>`): one target, no stacked
  backdrop texture, no resize plumbing; the source is shader-target
  compatible verbatim.
- `skybox.tsx` - a cube-map skybox (`<Scene background={{ cube,
  rotation }}>`) baked in JS at startup (horizon gradient, sun disc and
  glow, no image assets), and the same cube as the environment
  (`environment={{ cube, rotation }}`) mirrored by a chrome sphere
  (`lit({ reflectivity: 1 })`) and, blurred by shininess, a glossy knot.
  The sky turns and the sun light turns with it, the `rotation` knobs
  updating in place; drag to look around.
- `standard.tsx` - the `standard` material, the sphere grid every PBR
  engine opens with: metalness down the rows, roughness across the
  columns, under the skybox example's baked sky as background and
  environment and a warm sun casting shadows; a bare-metal row shows
  the environment as tinted reflections, the dielectric row the same
  sky as a faint face-on gloss. Drag to look around.
- `environment.tsx` - a baked HDR environment lighting the scene ALONE
  (no lights, no hemisphere): `loadEnvironment` reads the .srte that
  `bunx srt tool 3d/environment <panorama>.hdr -o assets/environment.srte`
  bakes from any equirectangular .hdr (Poly Haven's are CC0; the asset
  is not committed, bake one first), used as background and environment
  with ACES tone mapping. A metal row and a red dielectric row, roughness
  0 to 1 across: the metals are the room, sharp to blurred. Drag to
  look around.
- `probe.tsx` - a reflection probe (`scene.createReflectionProbe`): the
  scene rendered into a cube map from the center of a satin chrome ball,
  every frame, prefiltered into the roughness chain and set as the
  environment the ball reflects - six colored walls and four orbiting
  spheres show in it, blurred by the ball's roughness. Layers keep the
  ball out of its own probe. Drag to look around.
- `sky-lit.tsx` - a sky-lit scene (`scene.bakeBackground`): a procedural
  GLSL sky is the background AND, baked into a prefiltered cube, the
  environment - no light nodes. Two rows of `standard` spheres, metal and
  dielectric, roughness 0 to 1 across; the sun disc bakes at 40x (the
  probe format is half float where the device renders it), so the rough
  metals carry its energy as a broad highlight. Drag to look around.
- `scene-atlas.tsx` - four `<View3d>` tiled `into` one app-owned atlas
  (one pass for all four, `bufferFormat()` storage) and resolved once by
  the app: `resolveFragment()` over a `createShaderTexture` sampling the
  atlas, `resolveParams` for its tone mapping - the recipe for any buffer
  the library does not resolve itself.
- `bloom.tsx` - the stock `bloom` option on sky-lit's scene with the sun
  in frame (the disc and its mirror images bloom, the sky gradient does
  not), composed with a custom `resolve` that declares the chain's
  `uBloom`/`uBloomIntensity`, adds the term and a vignette, and ends in
  `resolveColor` - the two halves of the resolve slot. Drag to look
  around.
- `sprites.tsx` - sprites: a ring of `"full"` billboard glows that stay
  flat to the screen and `"fixed-y"` cutout trees that only yaw toward
  the camera and stay upright as it climbs, both turned in the vertex
  stage while the camera circles - no per-sprite JS per frame.
- `instanced.tsx` - record meshes, the JS-written population: one
  material class declaring `instanceAttributes`, two `<RecordMesh>`
  fleets (400 scattered rocks, a ring of pines) each ONE draw entry and
  ONE uModel, a spinning group moving both with two matrix writes, and
  `setRecordCount` from onFrame breathing the pine population. The class
  also declares `shadowVertex` (the placement math alone), so both
  fleets `castShadow` onto the lit ground - the pines' shadows breathe
  with them.
- `fleet.tsx` - instanced meshes, the node-backed population: one
  `<InstancedMesh>` of a thousand `<Instance>` nodes under the stock
  `lit` material with `instanceColors` (lighting, shadows, fog and a
  per-instance tint with no GLSL). Every few seconds a signal picks the
  next formation and the core springs every instance to its place
  through its `transition` - one signal write, zero JS per frame after
  it; a tap flips the struck instance's tint (pointer events name the
  instance; every style write between two frames is one buffer write).
  Drag to orbit; the `formation` and `state` debug commands drive it
  headlessly.
- `lod.tsx` - level of detail: a thousand trees as one `<InstancedLod>`
  (three entries, each instance at the level its own projected size
  picks) and a hero solid as a `<Lod>` of three `lodSize` meshes
  cross-fading through a dithered band, the camera flying over both
  with no per-frame JS; the levels are tinted so the hand-overs show.
- `first-person.tsx` - a first-person walk: `<FirstPersonCamera>` over
  a walled courtyard of shadow-casting pillars, driven by an input map
  the app binds (`firstPersonBindings`: WASD/arrows and the pad sticks
  to walk, the scene's drag to look on touch, mouse motion while the
  pointer is locked - click locks, Escape releases, the app's calls, not
  the control's); `clampPosition` keeps the walker inside the walls, the whole of
  the collision a camera control offers; the `pose` debug command reads
  and sets the pose headlessly.
- `collision.tsx` - collision without a physics engine: the same walker
  as a capsule through `moveAndSlide` over the scene's sweep queries,
  in a level of walls, a ramp, a platform with a ledge and pillars whose
  drawn meshes carry the collider layer bit too; gravity as a frame loop
  that runs only while airborne (Space jumps), pickups lit by one
  `overlap` per move. Debug commands `walk` (frame-sized steps through
  the collision), `jump`, `fall` (one airborne step by hand) and `state`
  drive it headlessly.
- `scene-views.tsx` - scene views: one scene rendered three times, the
  built-in perspective leaf plus two `scene.createView` targets - a
  top-down ORTHOGRAPHIC map (`ortho` on setCamera) and a side silhouette
  drawn with an `overrideMaterial`; one spinning group, one signal, every
  target fed by the core's one flush.
- `shadows.tsx` - directional shadow maps from three casting lights: a
  `castShadow` sun swinging through its arc (one setTransform per frame
  on the light node), a fixed cool fill and a low rim light, `castShadow`
  meshes turning in a group throwing three crossing shadows each, the
  ground and the casters receiving through plain `lit` (the default);
  each shadow camera follows its light's world matrix, each map is
  rendered by an internal view.
- `cascades.tsx` - cascaded shadow maps: a sun with `shadow: { cascades:
  3 }` over a field of pillars to the horizon under a slowly flying
  camera; three maps fitted to slices of the camera frustum, sampled
  tightest-first, so the shadows are sharp at the camera's feet and still
  there at the far edge of the ground. A click cycles 1..4 cascades (1 is
  the plain box widened to cover the field: one map's texels spread over
  it, blocky everywhere) and the `cascades`/`fly` debug commands set the
  count and the shadow distance and park the flight.
- `lamps.tsx` - spot and point lights in a dark courtyard: a warm SOFT
  spot (penumbra 0.4) swinging from a parent group, a fixed HARD spot
  (penumbra 0.05) aimed by `direction` at a knot on a pedestal, and a
  blue point bulb orbiting the crates; both spots `castShadow` (a
  perspective map each), and the header notes the decay-2 intensity
  scale (candela-like: a lamp 5 units up wants ~40, not ~2).
- `fog.tsx` - scene fog over a valley of pines between two ridges: a
  click cycles LINEAR (`{ near, far }`, a clear band then a fade to the
  far plane), EXP2 (`{ density }`, thickening from the first metre) and
  HEIGHT (`heightFalloff`: the valley floor fills, the hilltops and sky
  stay clear), then off; two suns show the material opt-out (`unlit({
  fog: false })` stays bright, its twin fogs). The `fog` debug command
  sets the mode and its knobs and `pan` parks the camera.
- `model.tsx` - a model from a file: `model.glb` (a small rover with
  nested node transforms, a mirrored node, a textured material, a
  transparent dome and a mesh without normals) parsed with `parseGltf`
  from a binary import and built by `createModel` into a Group of named
  parts; clicking a part hides it, clicking the body restores all.
- `wireframe.tsx` - the rover drawn solid, as a wireframe or as its
  feature edges: `wireframeGeometry`/`edgesGeometry` build "lines"
  geometry over each part's own vertices (one shared vertex upload, a
  second index buffer) and `setGeometry`/`setMaterial` swap it onto the
  live mesh, no node touched. Space, a tap on the rover, or the `wire`
  debug command (`{ mode: "solid" | "wireframe" | "edges" }`) cycles.
  Under it the debug helpers: `gridHelper` on the rover's floor and
  `axesHelper` at the origin through one `unlit({ vertexColors: true })`,
  and `box3Helper` around its bounds, turning with it.
- `model-load.tsx` - the same rover loaded ASYNC with `loadModel` from
  `assets/` under a `<Loading>` boundary: the async read lives in a memo,
  a second memo derives the scene JSX after that read, and the shell
  stays above the boundary - the two rules that avoid
  PENDING_ASYNC_UNTRACKED_READ and the suspend-retry element leak. Needs
  the asset baked into the running app:
  `bunx srt tool 3d/model examples/model.glb -o assets/model.srtm`.
