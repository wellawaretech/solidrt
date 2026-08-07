# @solidrt/3d

A retained 3D scene graph for SolidRT: meshes, materials and a camera,
declared as Solid components, rendered by the runtime into an ordinary
texture in your UI tree.

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
mesh costs one uniform write.

There is also an imperative layer underneath (`createScene`, `createMesh`,
`setTransform`, ...) usable without components, plus a small math module
(`@solidrt/3d/math`: column-major mat4, perspective, lookAt). For HUD
overlays, `scene.project(point)` maps a world point to scene pixels.

v1 scope: unlit color/textured materials plus `shaderMaterial` (your own
GLSL as a first-class material), geometry generators (box, plane, circle,
ring, sphere, cylinder, cone, torus, torus knot), one perspective camera
with an orbit control (`createOrbitCamera`: drag, zoom, auto-orbit).
Lights, transparency, model loading and picking are
staged next - see `okf/research/scene-graph-3d.md` for the roadmap. Full
usage notes and traps: [AGENTS.md](AGENTS.md); runnable examples:
[examples/](examples/).
