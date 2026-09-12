// Level of detail: a thousand trees as ONE instanced LOD - three draw
// entries whatever the spread, each instance drawing the level its own
// projected size picks - and one hero solid as a `<Lod>` group of three
// meshes cross-fading between levels. The choice is the spatial core's,
// per target, after each flush: the camera flies over the field and no
// per-frame JS touches a single tree.
//
// The levels are tinted on purpose (green, olive, blue) so the hand-overs
// show; a real forest would share one material. Sizes are projected
// heights as a fraction of the viewport, measured on the sphere around
// the object's box (half its diagonal, ~2.8 for a tree): under this 60
// degree camera a tree measures 1.73 * 2.8 / distance, so the thresholds
// below hand over at roughly 55 and 160 world units. The hero's `fade`
// widens each of its thresholds into a band where both levels draw with
// complementary dithers; the forest switches hard, with the core's
// hysteresis, since instanced levels never fade.
//
// Debug: `fly { t }` parks the camera at flight time t (seconds) so a
// snapshot is deterministic; `state` reports the camera and the tree
// count.
import { onFrame, pct, render } from "@solidrt/core"
import { addInstance, cone, cylinder, DirectionalLight, HemisphereLight, icosahedron, InstancedLod, phong, Lod, Mesh, mergeGeometries, plane, Scene, transformGeometry, useScene } from "@solidrt/3d"
import type { InstancedMeshNode } from "@solidrt/3d"
import { registerDebug } from "srt:dev"

const TREES = 1000
// Half the side of the square field the trees scatter over.
const FIELD = 110
// The hand-over sizes of the forest, nearest first (the last is the cull
// threshold: 0 keeps the far level drawn at any distance).
const FOREST_SIZES = [0.09, 0.03, 0]
// The hero's hand-over sizes and its cross-fade band fraction.
const HERO_SIZES = [0.45, 0.18, 0]
const HERO_FADE = 0.35

// A tree at three costs: foliage cone over a trunk, 24 / 7 / 3 segments.
function tree(segments: number): ReturnType<typeof mergeGeometries> {
  let foliage = transformGeometry(cone({ radius: 1.4, height: 3.2, radialSegments: segments }), { position: [0, 2.4, 0] })
  let trunk = transformGeometry(cylinder({ radiusTop: 0.22, radiusBottom: 0.3, height: 1.6, radialSegments: Math.max(3, segments >> 1) }), { position: [0, 0.8, 0] })
  return mergeGeometries([foliage, trunk], "tree-" + segments)
}

// A deterministic scatter (a small LCG), so every run is the same field.
function scatter(mesh: InstancedMeshNode): void {
  let seed = 12345
  let rand = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff
    return seed / 0x7fffffff
  }
  for (let i = 0; i < TREES; i++) {
    let x = (rand() * 2 - 1) * FIELD
    let z = (rand() * 2 - 1) * FIELD
    // Keep the hero's clearing empty.
    if (x * x + z * z < 12 * 12) continue
    let s = 0.7 + rand() * 0.6
    addInstance(mesh, { position: [x, 0, z], rotation: [0, rand() * Math.PI * 2, 0], scale: [s, s, s] })
  }
}

let parked: number | null = null
let flightTime = 0
let eye: [number, number, number] = [0, 0, 0]

// The flight: a slow orbit whose radius breathes between the clearing and
// the field's edge, height following the radius, always looking a little
// ahead of the hero.
function flyTo(setCamera: (u: { position: [number, number, number]; target: [number, number, number] }) => void, t: number): void {
  let radius = 14 + 80 * (0.5 - 0.5 * Math.cos(t * 0.22))
  let theta = t * 0.18
  eye = [radius * Math.cos(theta), 5 + radius * 0.22, radius * Math.sin(theta)]
  setCamera({ position: eye, target: [-8 * Math.sin(theta), 2, 8 * Math.cos(theta)] })
}

function Flight() {
  let { viewport } = useScene()
  flyTo(viewport.setCamera, 0)
  onFrame(tick => {
    flightTime = parked ?? tick / 1000
    flyTo(viewport.setCamera, flightTime)
  })
  return null
}

registerDebug("fly", (args?: Record<string, unknown>) => {
  parked = typeof args?.t === "number" ? args.t : null
  return { parked }
})
registerDebug("state", () => ({ t: flightTime, eye, trees: TREES }))

function App() {
  let hi = tree(24)
  let mid = tree(7)
  let lo = tree(3)
  let leafHi = phong({ color: [0.2, 0.55, 0.2], instanced: true })
  let leafMid = phong({ color: [0.5, 0.55, 0.15], instanced: true })
  let leafLo = phong({ color: [0.2, 0.35, 0.6], instanced: true })
  let ground = plane({ width: FIELD * 2.4, height: FIELD * 2.4 })
  return (
    <window>
      <Scene camera={{ fov: 60, near: 0.5, far: 400 }} clearColor={[0.62, 0.74, 0.9, 1]} fog={{ color: [0.62, 0.74, 0.9], near: 120, far: 320 }} label="lod">
        <Flight />
        <HemisphereLight sky={[0.7, 0.8, 1]} ground={[0.3, 0.28, 0.22]} intensity={0.9} />
        <DirectionalLight direction={[-0.5, -1, -0.3]} intensity={1.1} />
        <Mesh geometry={ground} material={phong({ color: [0.36, 0.42, 0.24] })} rotation={[-Math.PI / 2, 0, 0]} />
        <InstancedLod
          capacity={TREES}
          levels={[
            { geometry: hi, material: leafHi, size: FOREST_SIZES[0]! },
            { geometry: mid, material: leafMid, size: FOREST_SIZES[1]! },
            { geometry: lo, material: leafLo, size: FOREST_SIZES[2]! },
          ]}
          label="forest"
          ref={scatter}
        />
        <Lod fade={HERO_FADE} position={[0, 3.5, 0]}>
          <Mesh lodSize={HERO_SIZES[0]!} geometry={icosahedron({ radius: 3, detail: 3 })} material={phong({ color: [0.9, 0.35, 0.25], specular: 0.6 })} />
          <Mesh lodSize={HERO_SIZES[1]!} geometry={icosahedron({ radius: 3, detail: 1 })} material={phong({ color: [0.9, 0.6, 0.2], specular: 0.6 })} />
          <Mesh lodSize={HERO_SIZES[2]!} geometry={icosahedron({ radius: 3, detail: 0 })} material={phong({ color: [0.85, 0.8, 0.3] })} />
        </Lod>
      </Scene>
    </window>
  )
}

render(() => <App />)
