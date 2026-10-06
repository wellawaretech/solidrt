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
      <Scene>
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
as a plain `<texture>` leaf that fills its parent's box, rendered at the
leaf's on-screen device-pixel size (give `width`/`height` for a fixed
target instead); the `output` prop receives the texture id
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
per triangle, carrying the face, its uv and a normal facing the ray.
`scene.overlap`, `scene.sweep` and `scene.moveAndSlide` ask the same
index what a volume touches, where a moving one first touches, and how a
character body slides through it. A scene also takes a `background` -
fragment GLSL drawn inside its own pass behind the meshes, or a skybox
cube - replacing the stacked backdrop-texture pattern.
Custom materials get a standard uniform set - per-mesh `uModel`/`uNormal`,
shared `uViewProj`/`uCamPos`/`uCamRight`/`uCamUp`, each written once per
change - plus your own uniforms: scene-wide via `scene.setParams` (one write
however many meshes read it), or per mesh, declaratively via the `params`
prop on `<Mesh>` or imperatively via `setMeshParams`. `shaderMaterialClass`
compiles one program and hands out `instance()` materials that differ only
in params/textures - and with `instanceBuffers` it makes an instanced
material. Populations come in two forms: `<InstancedMesh>` with
`<Instance>` children (or `createInstancedMesh`/`addInstance`) draws the
geometry once per instance NODE - each a scene node the spatial core
places, so transitions, picking and pointer events reach every copy with
no per-frame JS, and the stock materials' `instanced`/`instanceColors`
give it lighting, shadows and a per-instance tint without GLSL
(`examples/fleet.tsx`); `<RecordMesh records>` (or `createRecordMesh`)
draws once per interleaved JS-written record, the shape for particles and
every fleet only JS can step (`examples/instanced.tsx`). And
`@solidrt/3d/glsl` exports the lighting pieces (hemisphere, lambert,
blinn, fresnel, a standard vertex stage) to compose your own lit looks
from plain template literals.

In the package: materials (`unlit`, Blinn-Phong `phong`, metal/rough
`standard` with surface maps and image-based lighting, `sprite` for
camera-facing quads with a screen-size clamp, and `shaderMaterial` for
your own GLSL as a first-class material), lights as graph nodes
(directional, spot, point and hemisphere, up to eight, with cascaded
directional, spot and six-face point shadows), geometry generators (box,
plane, circle, ring, sphere, capsule, cylinder, cone, torus, torus knot,
the platonic solids with `detail` subdivision, so the icosahedron is also
the icosphere), a profile kit for custom solids (`extrude` with bevels,
`lathe`, polyline `sweep`/`tube` with mitred joints, flat `polygon`, with
`fillet`/`roundRect`/`triangulate` helpers), geometry as data
(`transformGeometry` bakes a placement into vertices, `mergeGeometries`
concatenates parts, `withAttribute`/`withColors` append named channels
materials read by name, morph targets), models (`loadGltf`/`loadModel`,
skins, a clip mixer with root motion, a baked `.sol3m` container with
compressed textures), Gaussian splats, level of detail, views of one
scene (split screen, a minimap, a reflection probe), environment and sky
(skybox, equirect panoramas, baked environments, fog, bloom, tone
mapping), camera controls (`<OrbitCamera>` and `<FirstPersonCamera>`
driven by an input map the app binds, shots and blends between them),
picking with pointer events, and the collision queries. Full usage notes
and traps: [AGENTS.md](AGENTS.md); runnable examples:
[examples/](examples/); the ranked list of what is next:
`okf/notes/3d-roadmap.md`.
