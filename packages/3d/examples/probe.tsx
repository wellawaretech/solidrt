// A reflection probe: `scene.createReflectionProbe` renders the scene into
// a cube map from a point, and that cube is the environment a chrome ball
// at the same point mirrors - Three's CubeCamera, Unity's and Godot's
// realtime ReflectionProbe. Six colored walls make a room, four glowing
// spheres orbit the ball, and the probe re-renders every frame (six scene
// passes) so the orbit shows in the chrome. Layers keep the ball out of
// its own probe: the room and the orbiters live on layers 1 and 2, the
// ball on layer 1 only, and the probe looks at layer 2. Each update also
// prefilters the faces into the roughness chain (the default), so the
// ball's satin finish blurs the room the way a baked environment would;
// `prefilter: false` would keep a sharp mirror at a sixth of the passes
// (the chain is the frame's largest GPU item; see AGENTS.md). Drag to
// look around.

import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { DirectionalLight, HemisphereLight, Mesh, OrbitCamera, plane, Scene, sphere, standard, unlit } from "@solidrt/3d"
import type { ReflectionProbe, SceneHandle } from "@solidrt/3d"
import type { Vec3 } from "@solidrt/3d/math"

// Half the room's edge.
const ROOM = 5
// The orbiters' radius, orbit radius and speed (radians per second).
const ORBITER_RADIUS = 0.35
const ORBIT = 2
const ORBIT_SPEED = 0.8
// The chrome ball's radius and its satin roughness (0 is a mirror).
const BALL = 1
const BALL_ROUGHNESS = 0.2
// The probe's face edge.
const PROBE_SIZE = 128
// Layer bits: the scene camera sees both, the probe only ROOM_LAYER.
const BALL_LAYER = 1
const ROOM_LAYER = 2

// Wall colors (sRGB), one per face: +X, -X, +Y, -Y, +Z, -Z.
const WALL_COLORS: [number, number, number][] = [
  [0.75, 0.3, 0.25],
  [0.25, 0.6, 0.35],
  [0.85, 0.85, 0.9],
  [0.3, 0.28, 0.26],
  [0.25, 0.4, 0.75],
  [0.8, 0.7, 0.3],
]
// Each wall's position and the rotation that turns the plane's front inward.
const WALLS: { position: Vec3; rotation: Vec3 }[] = [
  { position: [ROOM, 0, 0], rotation: [0, -Math.PI / 2, 0] },
  { position: [-ROOM, 0, 0], rotation: [0, Math.PI / 2, 0] },
  { position: [0, ROOM, 0], rotation: [Math.PI / 2, 0, 0] },
  { position: [0, -ROOM, 0], rotation: [-Math.PI / 2, 0, 0] },
  { position: [0, 0, ROOM], rotation: [0, Math.PI, 0] },
  { position: [0, 0, -ROOM], rotation: [0, 0, 0] },
]
const ORBITER_COLORS: [number, number, number][] = [
  [1, 0.4, 0.2],
  [0.3, 0.9, 1],
  [1, 0.9, 0.3],
  [0.7, 0.4, 1],
]

function App() {
  let [t, setT] = createSignal(0)
  let scene: SceneHandle | undefined
  let probe: ReflectionProbe | undefined
  onFrame(tick => {
    setT(tick / 1000)
    if (scene !== undefined && probe === undefined) {
      probe = scene.createReflectionProbe({ position: [0, 0, 0], size: PROBE_SIZE, layers: ROOM_LAYER, label: "ball-probe" })
      scene.setEnvironment({ cube: probe.cube })
    }
    probe?.update()
  })
  let wall = plane({ width: ROOM * 2, height: ROOM * 2 })
  let walls = WALL_COLORS.map(color => unlit({ color }))
  let orbiter = sphere({ radius: ORBITER_RADIUS, widthSegments: 24, heightSegments: 16 })
  let orbiters = ORBITER_COLORS.map(color => unlit({ color }))
  let ball = standard({ color: [1, 1, 1], metalness: 1, roughness: BALL_ROUGHNESS })
  let orbitPosition = (i: number): Vec3 => {
    let a = t() * ORBIT_SPEED + (i * Math.PI) / 2
    return [Math.cos(a) * ORBIT, Math.sin(a * 1.7) * 0.8, Math.sin(a) * ORBIT]
  }
  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.05, 0.05, 0.06, 1]} samples={4} label="probe-demo" ref={s => (scene = s)}>
          <OrbitCamera target={[0, 0, 0]} azimuth={0.5} elevation={0.2} distance={4} />
          <HemisphereLight sky={[0.6, 0.65, 0.7]} ground={[0.3, 0.28, 0.26]} intensity={0.6} />
          <DirectionalLight direction={[0.4, -0.8, 0.3]} intensity={0.8} />
          {WALLS.map((w, i) => (
            <Mesh geometry={wall} material={walls[i]!} position={w.position} rotation={w.rotation} layers={BALL_LAYER | ROOM_LAYER} />
          ))}
          {ORBITER_COLORS.map((_, i) => (
            <Mesh geometry={orbiter} material={orbiters[i]!} position={orbitPosition(i)} layers={BALL_LAYER | ROOM_LAYER} />
          ))}
          <Mesh geometry={sphere({ radius: BALL, widthSegments: 64, heightSegments: 48 })} material={ball} layers={BALL_LAYER} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
