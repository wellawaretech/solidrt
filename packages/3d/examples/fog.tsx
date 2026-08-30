// Scene fog, all three forms over one valley: a camera on a hillside
// panning slowly across a floor of pines between two ridges, the far
// ridge near the camera's far plane. LINEAR fog (`{ near, far }`,
// Three's Fog) is a clear band then a fade, and hides the clip when
// `far` sits at the camera's; EXP2 (`{ density }`, Three's FogExp2 /
// Unity's default) thickens from the first metre with no band and
// never quite closes; HEIGHT (`heightFalloff` on either) fills the
// valley floor and thins on the way up, so the hilltops and the sky
// stay clear while the pines below drown - per fragment height, the
// cheap tier. Fog is one shared-params write per change, whatever the
// mesh count, and every standard material takes it; the two suns show
// the opt-out: the left one is `unlit({ fog: false })` and stays
// bright in every mode, the right one fogs like everything else.
//
// A click cycles linear -> exp2 -> height -> off. The `fog` debug
// command sets the mode and its knobs (`{ mode: "linear" | "exp2" |
// "height" | "off", near, far, density, height, falloff }`) and returns
// the state; `pan` parks the camera (`{ t: seconds }`), so a capture
// repeats.
import { createSignal, flush, onFrame, pct, render } from "@solidrt/core"
import { registerDebug } from "srt:dev"
import { cone, cylinder, DirectionalLight, HemisphereLight, lit, Mesh, PerspectiveCamera, plane, Scene, sphere, unlit } from "@solidrt/3d"
import type { FogOptions, Vec3 } from "@solidrt/3d"

const SIZE = 720
const FAR = 400
// The sky, shared by the clear and the fog so the horizon has no band.
const SKY: [number, number, number] = [0.72, 0.78, 0.86]
// The camera: on the near hillside, panning slowly across the valley.
const EYE: Vec3 = [-40, 42, 140]
const PAN_PERIOD = 60
// How far the look-at point swings left and right of the valley axis.
const PAN_SWING = 70
// The linear band, the exp2 thickness and the height layer.
const NEAR = 40
const LINEAR_FAR = FAR
const DENSITY = 0.006
const HEIGHT = 10
const HEIGHT_FALLOFF = 0.12

type Mode = "linear" | "exp2" | "height" | "off"
const MODES: Mode[] = ["linear", "exp2", "height", "off"]

let [mode, setMode] = createSignal<Mode>("linear")
let [near, setNear] = createSignal(NEAR)
let [far, setFar] = createSignal(LINEAR_FAR)
let [density, setDensity] = createSignal(DENSITY)
let [height, setHeight] = createSignal(HEIGHT)
let [falloff, setFalloff] = createSignal(HEIGHT_FALLOFF)
let [time, setTime] = createSignal(0)
let parked: number | null = null

let num = (v: unknown, set: (n: number) => void) => {
  if (typeof v === "number") set(v)
}

registerDebug("fog", (args?: Record<string, unknown>) => {
  if (typeof args?.mode === "string" && MODES.includes(args.mode as Mode)) setMode(args.mode as Mode)
  num(args?.near, setNear)
  num(args?.far, setFar)
  num(args?.density, setDensity)
  num(args?.height, setHeight)
  num(args?.falloff, setFalloff)
  flush()
  return { mode: mode(), near: near(), far: far(), density: density(), height: height(), falloff: falloff() }
})
registerDebug("pan", (args?: Record<string, unknown>) => {
  if (typeof args?.t === "number") {
    parked = args.t
    setTime(args.t)
  } else if (args?.t === null) {
    parked = null
  }
  flush()
  return { t: time(), parked: parked !== null }
})

function fog(): FogOptions | undefined {
  switch (mode()) {
    case "linear":
      return { color: SKY, near: near(), far: far() }
    case "exp2":
      return { color: SKY, density: density() }
    case "height":
      return { color: SKY, density: density(), height: height(), heightFalloff: falloff() }
    case "off":
      return undefined
  }
}

// The two ridges: flattened spheres sunk into the ground, and the height
// they give the ground at (x, z) so the pines stand on them.
type Hill = { position: Vec3; scale: Vec3 }
let hills: Hill[] = [
  { position: [-90, -8, 120], scale: [110, 44, 90] },
  { position: [60, -6, -140], scale: [150, 40, 90] },
  { position: [-140, -10, -60], scale: [90, 36, 120] },
  { position: [150, -4, 40], scale: [80, 26, 70] },
]
function groundHeight(x: number, z: number): number {
  let y = 0
  for (let h of hills) {
    let dx = (x - h.position[0]) / h.scale[0]
    let dz = (z - h.position[2]) / h.scale[2]
    let r = 1 - dx * dx - dz * dz
    if (r > 0) y = Math.max(y, h.position[1] + h.scale[1] * Math.sqrt(r))
  }
  return y
}

// The pines, on a jittered grid over the valley and up the ridges.
type Pine = { position: Vec3; size: number }
let pines: Pine[] = []
let step = 18
for (let i = -12; i <= 12; i++) {
  for (let j = -12; j <= 12; j++) {
    let x = i * step + ((i * 7 + j * 13) % 9) - 4
    let z = j * step + ((i * 11 + j * 5) % 9) - 4
    let y = groundHeight(x, z)
    // No pines on the near hillside under the camera, none above the tree line.
    if (y > 30 || (x < -20 && z > 90)) continue
    let size = 5 + ((i * 3 + j * 5 + 20) % 5)
    pines.push({ position: [x, y, z], size })
  }
}

function App() {
  onFrame(tick => {
    if (parked === null) setTime(tick / 1000)
  })
  let lookAt = () => {
    let a = Math.sin((time() / PAN_PERIOD) * 2 * Math.PI)
    return [a * PAN_SWING, 6, -60] as Vec3
  }

  let ground = lit({ color: [0.36, 0.48, 0.3] })
  let rock = lit({ color: [0.5, 0.47, 0.42] })
  let needles = lit({ color: [0.16, 0.36, 0.22] })
  let trunk = lit({ color: [0.35, 0.25, 0.16] })
  let sunLit = unlit({ color: [1, 0.92, 0.6], fog: false })
  let sunFogged = unlit({ color: [1, 0.92, 0.6] })

  let floor = plane({ width: 800, height: 800 })
  let hill = sphere({ radius: 1, widthSegments: 48, heightSegments: 24 })
  let crown = cone({ radius: 0.5, height: 1, radialSegments: 10 })
  let stem = cylinder({ radiusTop: 0.08, radiusBottom: 0.1, height: 1, radialSegments: 6 })
  let sun = sphere({ radius: 14, widthSegments: 24, heightSegments: 12 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} designSize={[SIZE, SIZE]} onPointerDown={() => setMode(m => MODES[(MODES.indexOf(m) + 1) % MODES.length] ?? "linear")}>
        <Scene width={SIZE} height={SIZE} clearColor={[SKY[0], SKY[1], SKY[2], 1]} fog={fog()} samples={4} label="fog">
          <PerspectiveCamera fov={55} near={0.5} far={FAR} position={EYE} lookAt={lookAt()} />
          <HemisphereLight sky={[0.55, 0.62, 0.75]} ground={[0.28, 0.24, 0.2]} />
          <DirectionalLight color={[0.9, 0.85, 0.75]} rotation={[-0.9, 0.6, 0]} />
          <Mesh geometry={floor} material={ground} rotation={[-Math.PI / 2, 0, 0]} />
          {hills.map(h => (
            <Mesh geometry={hill} material={rock} position={h.position} scale={h.scale} />
          ))}
          {pines.map(p => (
            <>
              <Mesh geometry={stem} material={trunk} position={[p.position[0], p.position[1] + p.size * 0.15, p.position[2]]} scale={[p.size, p.size * 0.3, p.size]} />
              <Mesh geometry={crown} material={needles} position={[p.position[0], p.position[1] + p.size * 0.3 + p.size * 0.5, p.position[2]]} scale={[p.size * 0.9, p.size, p.size * 0.9]} />
            </>
          ))}
          <Mesh geometry={sun} material={sunLit} position={[-30, 100, -220]} />
          <Mesh geometry={sun} material={sunFogged} position={[30, 100, -220]} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
