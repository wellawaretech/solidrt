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
- `sprites.tsx` - sprites: a ring of `"full"` billboard glows that stay
  flat to the screen and `"fixed-y"` cutout trees that only yaw toward
  the camera and stay upright as it climbs, both turned in the vertex
  stage while the camera circles - no per-sprite JS per frame.
- `instanced.tsx` - instanced meshes: one material class declaring
  `instanceAttributes`, two `<InstancedMesh>` fleets (400 scattered
  rocks, a ring of pines) each ONE draw entry and ONE uModel, a spinning
  group moving both with two matrix writes, and `setInstanceCount` from
  onFrame breathing the pine population.
- `model.tsx` - a model from a file: `model.glb` (a small rover with
  nested node transforms, a mirrored node, a textured material, a
  transparent dome and a mesh without normals) parsed with `parseGltf`
  from a binary import and built by `createModel` into a Group of named
  parts; clicking a part hides it, clicking the body restores all.
