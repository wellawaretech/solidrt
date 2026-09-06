// A fleet of a thousand instance NODES: one `<InstancedMesh>` under the
// stock `lit` material with `instanceColors`, so lighting, shadows, fog
// and a per-instance tint come with no GLSL at all. Each `<Instance>`
// declares a spring `transition`; every few seconds a signal picks the
// next formation (grid, rings, shell, helix), the thousand `position`
// props update, and the spatial core springs every instance to its new
// place - the change is one signal write, and the frames after it cost
// no JS at all (the core writes the matrix records itself). A tap on an
// instance flips its tint: pointer events name the instance struck, and
// however many style records change between two frames they land as ONE
// buffer write. Drag to orbit.
//
// Debug commands: `formation` ({ name } sets one and parks the cycle,
// { cycle: true } resumes; returns the current one), `state` (formation,
// the last tapped instance, how many are flipped) and `pixel` ({ i }: the
// scene pixel instance i projects to and what a pick there finds, for a
// synthetic tap).
import { createSignal, flush, For, pct, render } from "@solidrt/core"
import { box, DirectionalLight, HemisphereLight, Instance, InstancedMesh, lit, Mesh, OrbitCamera, plane, Scene, worldPosition } from "@solidrt/3d"
import type { InstanceNode, NodeTransition, SceneHandle, Vec3 } from "@solidrt/3d"
import { registerDebug } from "srt:dev"

type Tint = [number, number, number, number]

const COUNT = 1000
// Milliseconds between formation changes.
const CYCLE_MS = 4000
// The spring every instance moves by: about a second to settle, a small
// overshoot so the fleet visibly swings into place.
const SPRING: NodeTransition = { position: { duration: 900, bounce: 0.2 } }
// Edge of one unit cube, and the half-width of the ground the formations
// fit in.
const UNIT = 0.18
const REACH = 5
const SKY: [number, number, number] = [0.07, 0.08, 0.1]
// The tint a tapped instance flips to.
const HOT: Tint = [1, 0.35, 0.2, 1]

// Formations: where instance i of COUNT stands. Each is a pure function
// of the index, so a formation change is a signal write and nothing else.
const GRID_COLUMNS = 40
const RING_SIZE = 100
const RING_STEP = 0.4
const SHELL_RADIUS = 3
const SHELL_HEIGHT = 3.5
const HELIX_TURNS = 12
const HELIX_HEIGHT = 6
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5))
const FORMATIONS: { name: string; at: (i: number) => Vec3 }[] = [
  {
    name: "grid",
    at: i => {
      let spacing = (2 * REACH) / GRID_COLUMNS
      let rows = COUNT / GRID_COLUMNS
      return [((i % GRID_COLUMNS) - (GRID_COLUMNS - 1) / 2) * spacing, UNIT / 2, (Math.floor(i / GRID_COLUMNS) - (rows - 1) / 2) * spacing]
    },
  },
  {
    name: "rings",
    at: i => {
      let ring = Math.floor(i / RING_SIZE)
      let a = ((i % RING_SIZE) / RING_SIZE) * Math.PI * 2 + ring * 0.1
      let r = 1.2 + ring * RING_STEP
      return [Math.cos(a) * r, UNIT / 2, Math.sin(a) * r]
    },
  },
  {
    name: "shell",
    at: i => {
      // A Fibonacci sphere: evenly spread points on the shell.
      let y = 1 - (2 * (i + 0.5)) / COUNT
      let r = Math.sqrt(1 - y * y)
      let a = i * GOLDEN_ANGLE
      return [Math.cos(a) * r * SHELL_RADIUS, SHELL_HEIGHT + y * SHELL_RADIUS, Math.sin(a) * r * SHELL_RADIUS]
    },
  },
  {
    name: "helix",
    at: i => {
      let t = i / COUNT
      let a = t * HELIX_TURNS * Math.PI * 2
      return [Math.cos(a) * SHELL_RADIUS, UNIT / 2 + t * HELIX_HEIGHT, Math.sin(a) * SHELL_RADIUS]
    },
  },
]

// The fleet's resting tint: a gradient along the index, so the formations
// read as one ribbon folding into the next shape.
function gradient(i: number): Tint {
  let t = i / COUNT
  return [0.35 + 0.5 * t, 0.55, 0.9 - 0.6 * t, 1]
}

const INDICES = Array.from({ length: COUNT }, (_, i) => i)

let [formation, setFormation] = createSignal(0)
let tints = INDICES.map(i => createSignal<Tint>(gradient(i)))
let flipped = new Set<number>()
let lastTapped: number | null = null
let parked = false
// The handles the debug commands read: the scene and each instance node.
let scene: SceneHandle | undefined
let nodes: InstanceNode[] = []

let flip = (i: number) => {
  let [, setTint] = tints[i]!
  if (flipped.has(i)) {
    flipped.delete(i)
    setTint(gradient(i))
  } else {
    flipped.add(i)
    setTint(HOT)
  }
  lastTapped = i
  console.log(`tap instance ${i}`)
}

setInterval(() => {
  if (!parked) setFormation(f => (f + 1) % FORMATIONS.length)
}, CYCLE_MS)

registerDebug("formation", (args?: Record<string, unknown>) => {
  if (typeof args?.name === "string") {
    let k = FORMATIONS.findIndex(f => f.name === args.name)
    if (k >= 0) {
      setFormation(k)
      parked = true
    }
  }
  if (args?.cycle === true) parked = false
  flush()
  return { formation: FORMATIONS[formation()]!.name, parked }
})
registerDebug("state", () => ({ formation: FORMATIONS[formation()]!.name, tapped: lastTapped, flipped: flipped.size }))
registerDebug("pixel", (args?: Record<string, unknown>) => {
  let node = typeof args?.i === "number" ? nodes[args.i] : undefined
  if (node === undefined || scene === undefined) return null
  let at = scene.project(worldPosition(node))
  if (at === null) return null
  // What a pick at that pixel finds: the instance's slot, or the mesh kind.
  let hit = scene.pick(at.x, at.y)[0]
  return { x: at.x, y: at.y, hit: hit === undefined ? null : (hit.instance?._slot ?? hit.mesh.kind) }
})

function App() {
  let place = (i: number): Vec3 => FORMATIONS[formation()]!.at(i)
  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[SKY[0], SKY[1], SKY[2], 1]} fog={{ color: SKY, near: 12, far: 30 }} label="fleet" ref={s => (scene = s)}>
          <OrbitCamera target={[0, 2, 0]} distance={15} elevation={0.45} azimuth={0.6} />
          <HemisphereLight sky={[0.45, 0.47, 0.5]} ground={[0.12, 0.13, 0.11]} />
          <DirectionalLight
            color={[1, 0.95, 0.85]}
            intensity={0.8}
            position={[8, 14, 6]}
            direction={[-8, -14, -6]}
            castShadow
            shadow={{
              normalBias: 0.03,
              camera: { left: -REACH - 1, right: REACH + 1, top: REACH + 3, bottom: -REACH - 3, near: 1, far: 40 },
            }}
          />
          <Mesh
            geometry={plane({ width: 3 * REACH, height: 3 * REACH, label: "ground" })}
            material={lit({ color: [0.26, 0.28, 0.27] })}
            rotation={[-Math.PI / 2, 0, 0]}
          />
          <InstancedMesh
            geometry={box({ width: UNIT, height: UNIT, depth: UNIT, label: "unit" })}
            material={lit({ instanceColors: true, specular: 0.3, shininess: 40 })}
            capacity={COUNT}
            castShadow
            label="fleet"
          >
            <For each={INDICES}>
              {i => (
                <Instance
                  position={place(i)}
                  style={tints[i]![0]()}
                  transition={SPRING}
                  onPointerDown={() => flip(i)}
                  ref={node => (nodes[i] = node)}
                />
              )}
            </For>
          </InstancedMesh>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
