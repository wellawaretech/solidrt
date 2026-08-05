# @solidrt/3d examples

One concept per file; run with `bunx srt run <file>` from an app that
depends on `@solidrt/3d` (or in-repo from the package directory).

- `scene-basic.tsx` - the whole v1 surface: a `<Scene>` composited as a
  texture leaf, `<PerspectiveCamera>`, a ground plane, a spinning
  `<Group>` of unlit meshes with real depth-buffer occlusion, geometry
  and pipeline sharing, and the one-signal onFrame drive.
