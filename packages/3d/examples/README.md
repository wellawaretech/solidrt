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
- `pick.tsx` - mesh pointer events: hover tints (enter/leave), click
  pops (down), a Group hearing its children's clicks through bubbling
  and one mesh stopping the walk; a STATIC scene rendered only when an
  event changes something, with hit testing over the scene's BVH.
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
- `sprites.tsx` - sprites: a ring of `"full"` billboard glows that stay
  flat to the screen and `"fixed-y"` cutout trees that only yaw toward
  the camera and stay upright as it climbs, both turned in the vertex
  stage while the camera circles - no per-sprite JS per frame.
- `instanced.tsx` - instanced meshes: one material class declaring
  `instanceAttributes`, two `<InstancedMesh>` fleets (400 scattered
  rocks, a ring of pines) each ONE draw entry and ONE uModel, a spinning
  group moving both with two matrix writes, and `setInstanceCount` from
  onFrame breathing the pine population. The class also declares
  `shadowVertex` (the placement math alone), so both fleets `castShadow`
  onto the lit ground - the pines' shadows breathe with them.
- `first-person.tsx` - a first-person walk: `<FirstPersonCamera>` over
  a walled courtyard of shadow-casting pillars, WASD/arrows and the pad
  sticks to walk, mouse look under pointer lock (click locks, Escape
  releases - the app's calls, not the control's), a drag to look on
  touch; `clampPosition` keeps the walker inside the walls, the whole of
  the collision a camera control offers; the `pose` debug command reads
  and sets the pose headlessly.
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
- `model-load.tsx` - the same rover loaded ASYNC with `loadModel` from
  `assets/` under a `<Loading>` boundary: the async read lives in a memo,
  a second memo derives the scene JSX after that read, and the shell
  stays above the boundary - the two rules that avoid
  PENDING_ASYNC_UNTRACKED_READ and the suspend-retry element leak. Needs
  the asset baked into the running app:
  `bunx srt tool 3d/model examples/model.glb -o assets/model.srtm`.
