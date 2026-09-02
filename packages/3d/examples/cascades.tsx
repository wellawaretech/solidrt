// Cascaded shadow maps: a field of pillars to the horizon under a slowly
// flying camera, lit by one casting sun with `shadow: { cascades: 3 }`.
// A box shadow (`shadow.camera`) is one map over a fixed area: widened to
// cover this field its 1024 texels spread over 260 world units and every
// shadow is blocky, kept tight the far field is unshadowed. With cascades
// the light renders one map per slice of the SCENE camera's frustum
// (near..far, tightest first), re-fitted every time the camera or the
// light moves, and a receiver samples the tightest map that has the
// point: sharp contact shadows at the camera's feet, coarser ones toward
// the horizon, at the cost of N times the shadow fill. Every map is a
// tile of the scene's one atlas, so the pass count is unchanged.
//
// A click cycles 1..4 cascades (1 = the box, widened over the field).
// The `cascades` debug command sets the count and the shadow distance
// (`{ count, distance }`; the range the cascades split, the camera's far
// by default - pulling it in sharpens every cascade) and `fly` parks the
// flight (`{ t: seconds }`), so a capture repeats. The field is fogged
// toward the sky color from FOG_NEAR to FOG_FAR, inside the camera's far
// plane, so the far pillars sink into the horizon instead of clipping
// (`examples/fog.tsx` is the fog tour).
import { createSignal, flush, onFrame, pct, render } from "@solidrt/core"
import { registerDebug } from "srt:dev"
import { box, DirectionalLight, HemisphereLight, lit, Mesh, PerspectiveCamera, plane, Scene, sphere } from "@solidrt/3d"
import type { Geometry, Vec3 } from "@solidrt/3d"

const FIELD = 260
const FAR = 200
// The flight: a circle over the field, looking ahead along it.
const RADIUS = 50
const HEIGHT = 5
const PERIOD = 90
// The sky, shared by the clear and the fog so the horizon has no band.
const SKY: [number, number, number] = [0.6, 0.72, 0.88]
// The fog band: clear up to FOG_NEAR, fully sky at FOG_FAR (the far edge
// of the field is about 150 units out at the flight's radius).
const FOG_NEAR = 30
const FOG_FAR = 150

let [cascades, setCascades] = createSignal(3)
let [distance, setDistance] = createSignal<number | null>(null)
// The flight clock, in seconds; `parked` holds it.
let [time, setTime] = createSignal(0)
let parked: number | null = null

registerDebug("cascades", (args?: Record<string, unknown>) => {
  if (typeof args?.count === "number") setCascades(args.count)
  if (typeof args?.distance === "number" || args?.distance === null) setDistance(args.distance)
  flush()
  return { cascades: cascades(), distance: distance() }
})
registerDebug("fly", (args?: Record<string, unknown>) => {
  if (typeof args?.t === "number") {
    parked = args.t
    setTime(args.t)
  } else if (args?.t === null) {
    parked = null
  }
  flush()
  return { t: time(), parked: parked !== null }
})

// A grid of pillars with a sphere on every third one, heights varying so
// the shadows differ in length; one geometry per height, shared.
let pillars: { position: Vec3; height: number; ball: boolean }[] = []
let step = 16
for (let i = -7; i <= 7; i++) {
  for (let j = -7; j <= 7; j++) {
    let h = 2 + ((i * 7 + j * 3 + 100) % 5)
    pillars.push({ position: [i * step + (j % 2) * 4, h / 2, j * step + (i % 2) * 5], height: h, ball: (i + j) % 3 === 0 })
  }
}
let boxes = new Map<number, Geometry>()
let pillar = (height: number): Geometry => {
  let g = boxes.get(height)
  if (g === undefined) boxes.set(height, (g = box({ width: 1.2, height, depth: 1.2 })))
  return g
}
let orb = sphere({ radius: 0.9 })

function App() {
  onFrame(tick => {
    if (parked === null) setTime(tick / 1000)
  })
  let eye = (): Vec3 => {
    let a = (time() / PERIOD) * Math.PI * 2
    return [Math.sin(a) * RADIUS, HEIGHT, Math.cos(a) * RADIUS]
  }
  let ahead = (): Vec3 => {
    let a = (time() / PERIOD) * Math.PI * 2 + 0.5
    return [Math.sin(a) * RADIUS, 1.5, Math.cos(a) * RADIUS]
  }

  let ground = lit({ color: [0.5, 0.55, 0.45] })
  let stone = lit({ color: [0.75, 0.7, 0.62] })
  let ball = lit({ color: [0.85, 0.35, 0.3], specular: 0.4, shininess: 30 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} onPointerDown={() => setCascades(c => (c % 4) + 1)}>
        <Scene
          clearColor={[SKY[0], SKY[1], SKY[2], 1]}
          fog={{ color: SKY, near: FOG_NEAR, far: FOG_FAR }}
          samples={4}
          label="cascades"
        >
          <PerspectiveCamera fov={50} near={0.5} far={FAR} position={eye()} lookAt={ahead()} />
          <HemisphereLight sky={[0.5, 0.58, 0.7]} ground={[0.25, 0.22, 0.18]} />
          <DirectionalLight
            color={[1, 0.95, 0.85]}
            intensity={0.9}
            position={[40, 90, 30]}
            direction={[-1, -0.55, -0.4]}
            castShadow
            shadow={{
              mapSize: 1024,
              normalBias: 0.08,
              cascades: cascades(),
              distance: distance(),
              // The box tier's frustum, when cascades is 1: the whole field.
              camera: { left: -FIELD / 2, right: FIELD / 2, top: FIELD / 2, bottom: -FIELD / 2, near: 1, far: 400 },
            }}
          />
          <Mesh geometry={plane({ width: FIELD, height: FIELD })} material={ground} rotation={[-Math.PI / 2, 0, 0]} />
          {pillars.map(p => (
            <>
              <Mesh geometry={pillar(p.height)} material={stone} position={p.position} castShadow />
              {p.ball ? <Mesh geometry={orb} material={ball} position={[p.position[0], p.height + 0.9, p.position[2]]} castShadow /> : null}
            </>
          ))}
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
