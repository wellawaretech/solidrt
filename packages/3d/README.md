# @solidrt/3d

A retained 3D scene graph for SolidRT: meshes, materials and a camera,
declared as Solid components, rendered by the runtime into an ordinary
texture in your UI tree.

_@solidrt/3d is experimental: expect more API churn here than in the rest of SolidRT._

```tsx
import { createSignal, onFrame, render } from "@solidrt/core"
import { box, Mesh, PerspectiveCamera, Scene, unlit } from "@solidrt/3d"

function App() {
  let [spin, setSpin] = createSignal(0)
  onFrame(tick => setSpin(tick / 2000))
  return (
    <window>
      <Scene width={720} height={720}>
        <PerspectiveCamera position={[0, 1.5, 3]} lookAt={[0, 0, 0]} />
        <Mesh geometry={box()} material={unlit({ color: [0.9, 0.3, 0.3] })} rotation={[0, spin(), 0]} />
      </Scene>
    </window>
  )
}
render(() => <App />)
```

The scene compiles to one depth-buffered GPU draw target: one draw entry
per mesh, one shared pipeline per material class, cross-mesh occlusion
from the shared depth buffer. A static scene costs zero GPU passes - the
runtime re-renders the target only when something changes - and a moved
mesh costs one uniform write. By default `<Scene>` composites the target
as a plain `<texture>` leaf; the `output` prop receives the texture id
and replaces that leaf - place a `<d-texture>`, add paint or pointer
props, chain a post-effect shader target, or return null and composite
`scene.texture` yourself.

There is also an imperative layer underneath (`createScene`, `createMesh`,
`setTransform`, ...) usable without components, plus a small math module
(`@solidrt/3d/math`: column-major mat4, perspective, lookAt). To aim a
node, `lookAt(node, target, up?)` points its local +z at a world point,
Three's `Object3D.lookAt`; `worldPosition(node)` is the companion for
aiming along a direction, and `quatFromTo` aims any other axis. For HUD
overlays, `scene.project(point)` maps a world point to scene pixels.

Rotation is stored as a quaternion (`quaternion` prop, `node.quaternion`),
so aiming and interpolation are gimbal-free and there is no second
rotation field to fall out of step. Euler triples stay the easy way to
author one - the `rotation` prop takes radians in XYZ order, matching
Three's `Euler` default, and `getRotation(node)` reads one back. The
verbs: `quatFromAxisAngle`, `quatMultiply`, and `quatSlerp` (smooth
tracking, damped follows) round out `quatFromTo`; `examples/aim.tsx`
shows each aiming style live.

Meshes take pointer events like elements do: `onPointerDown/Move/Up/
Enter/Leave` props on `<Mesh>` and `<Group>`, with bubbling, capture on
drag, and hover enter/leave pairs - hit testing runs over a BVH the
scene maintains incrementally, so events put no ceiling on scene size.
Underneath sit `scene.pick(x, y)` (the camera ray through a pixel,
`project()`'s inverse) and `scene.raycast(origin, direction)`; hits are
bounding-box accurate in v1. A scene also takes a `background` - fragment
GLSL drawn inside its own pass behind the meshes, replacing the stacked
backdrop-texture pattern.
Custom materials get a standard uniform set - per-mesh `uModel`/`uNormal`,
shared `uViewProj`/`uCamPos`/`uCamRight`/`uCamUp`, each written once per
change - plus your own uniforms: scene-wide via `scene.setParams` (one write
however many meshes read it), or per mesh, declaratively via the `params`
prop on `<Mesh>` or imperatively via `setMeshParams`. `shaderMaterialClass`
compiles one program and hands out `instance()` materials that differ only
in params/textures. And
`@solidrt/3d/glsl` exports the lighting pieces (hemisphere, lambert,
blinn, fresnel, a standard vertex stage) to compose your own lit looks
from plain template literals.

v1 scope: unlit color/textured materials plus `shaderMaterial` (your own
GLSL as a first-class material), geometry generators (box, plane, circle,
ring, sphere, cylinder, cone, torus, torus knot), a profile kit for custom
solids (`extrude` with bevels, `lathe`, polyline `sweep`/`tube` with
mitred joints, flat `shape`, with `fillet`/`roundRect`/`triangulate`
helpers), a per-vertex data channel
(`withColors` adds an `aColor` vec4 - tint, baked AO, any four scalars -
to any geometry, for materials that read it), one perspective camera
with an orbit control (`createOrbitCamera`: drag, pinch/wheel zoom, auto-orbit),
mesh picking with pointer events, and scene backgrounds.
Lights, transparency and model loading are
staged next - see `okf/research/scene-graph-3d.md` for the roadmap. Full
usage notes and traps: [AGENTS.md](AGENTS.md); runnable examples:
[examples/](examples/).
