# @solidrt/3d examples

One concept per file; run with `bunx srt run <file>` from an app that
depends on `@solidrt/3d` (or in-repo from the package directory).

- `scene-basic.tsx` - the whole v1 surface: a `<Scene>` composited as a
  texture leaf, `<PerspectiveCamera>`, a ground plane, a spinning
  `<Group>` of unlit meshes with real depth-buffer occlusion, geometry
  and pipeline sharing, and the one-signal onFrame drive.
- `sweep-paths.tsx` - swept solids along polylines: a flat strap folding
  over a crate (bare path points crease on the mitred bends) and a coiled
  tube (smooth-tagged helix, one continuous mesh), lit via the exported
  GLSL so the creased-vs-smooth normals actually show.
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
