// Collision without a physics engine: a capsule-controlled first-person
// walker in a level of walls, a ramp, a platform with a ledge and a few
// pillars, all colliding through `moveAndSlide` over the scene's sweep
// queries. The level meshes carry the COLLIDER layer bit beside the drawn
// one, so the same geometry draws and collides (a low-poly stand-in would
// carry COLLIDER alone and stay undrawn); the pickups do not, so the
// walker passes through them, and one `overlap` per move lights the ones
// within reach. Walking goes through the camera's `clampPosition` hook;
// gravity runs a frame loop only while airborne (a jump, a ledge), so a
// standing scene renders nothing new. WASD/arrows walk, Space jumps,
// click locks the pointer, Escape releases it. Debug commands: `pose`
// (read/set), `walk` ({ dx, dz } through the collision), `jump`, `fall`
// ({ dt }: one airborne step by hand, for headless checks), `state`.
import { createSignal, lockPointer, onFrame, pct, pointerLocked, render } from "@solidrt/core"
import type { PointerEvent } from "@solidrt/core"
import {
  box,
  cylinder,
  DirectionalLight,
  FirstPersonCamera,
  HemisphereLight,
  lit,
  Mesh,
  moveAndSlide,
  PerspectiveCamera,
  plane,
  Scene,
  sphere,
} from "@solidrt/3d"
import type { FirstPersonCameraHandle, MeshNode, MoveResult, SceneHandle, Vec3, Volume } from "@solidrt/3d"
import { registerDebug } from "srt:dev"

// Layer bit the level geometry adds to its drawn layer: what every
// collision query selects, so pickups (drawn only) never block the walker.
const COLLIDER = 2
// The walker: eye height over the feet, the capsule's radius, and where
// its segment sits relative to the eye (feet + radius up to a hand under
// the eye).
const EYE = 1.6
const RADIUS = 0.3
const BODY_LOW = -EYE + RADIUS
const BODY_HIGH = -0.2
// Vertical dynamics, world units per second (squared for gravity).
const GRAVITY = 12
const JUMP_SPEED = 5
// Largest frame step the fall integrates: a stall must not tunnel the
// walker through a floor.
const MAX_FALL_DT = 0.05
// A pickup lights up within this distance of the feet.
const REACH = 1.5
// Level dimensions: floor half extent, wall height, the platform's top
// and footprint, the ramp's run.
const FLOOR = 15
const WALL_HEIGHT = 3
const PLATFORM_TOP = 1.5
const PLATFORM_HALF = 3
const RAMP_RUN = 6
const RAMP_THICKNESS = 0.3
// HUD refresh interval in ms.
const HUD_MS = 100
// One frame of walking at the default speed, the stride the `walk` debug
// command takes so its checks match live play.
const WALK_STEP = 0.05

let bodyAt = (eye: Vec3): Volume => ({ a: [eye[0], eye[1] + BODY_LOW, eye[2]], b: [eye[0], eye[1] + BODY_HIGH, eye[2]], radius: RADIUS })
let feetOf = (eye: Vec3): Vec3 => [eye[0], eye[1] - EYE, eye[2]]

function App() {
  let scene!: SceneHandle
  let camera!: FirstPersonCameraHandle
  let [hud, setHud] = createSignal("")
  let [near, setNear] = createSignal<Set<number>>(new Set())
  // The same set, readable synchronously (a signal read right after its
  // write still returns the old value): what the debug commands report.
  let nearNow = new Set<number>()
  let grounded = true
  let fallSpeed = 0
  let stopFall: (() => void) | null = null
  let pickups: MeshNode[] = []

  let dim = lit({ color: [0.55, 0.45, 0.2] })
  let bright = lit({ color: [1, 0.85, 0.3], specular: 0.8, shininess: 60 })
  let ground = lit({ color: [0.48, 0.52, 0.46] })
  let stone = lit({ color: [0.72, 0.7, 0.66] })
  let brick = lit({ color: [0.6, 0.34, 0.28] })

  // Every move ends here: the walker's new eye, the ground state, and the
  // pickups in reach of its feet (an include-list query, so the level
  // itself never reports).
  let settle = (eye: Vec3, floor: Vec3 | null) => {
    grounded = floor !== null
    let hit = new Set<number>()
    for (let o of scene.overlap({ center: feetOf(eye), radius: REACH }, { meshes: pickups })) hit.add(pickups.indexOf(o.mesh))
    nearNow = hit
    setNear(hit)
  }
  // Walking: the camera asks for `next`, the collision answers.
  let clamp = (next: Vec3): Vec3 => {
    let eye = camera.pose().position
    let r = moveAndSlide(scene, bodyAt(eye), [next[0] - eye[0], next[1] - eye[1], next[2] - eye[2]], { layers: COLLIDER })
    let moved: Vec3 = [eye[0] + r.motion[0], eye[1] + r.motion[1], eye[2] + r.motion[2]]
    settle(moved, r.floor)
    if (!grounded) fall()
    return moved
  }
  // One airborne step: gravity into the vertical speed, the drop through
  // the collision, a ceiling bonk kills the rise, a floor ends the fall.
  let fallStep = (dt: number) => {
    fallSpeed -= GRAVITY * dt
    let eye = camera.pose().position
    let r = moveAndSlide(scene, bodyAt(eye), [0, fallSpeed * dt, 0], { layers: COLLIDER })
    let moved: Vec3 = [eye[0] + r.motion[0], eye[1] + r.motion[1], eye[2] + r.motion[2]]
    camera.set({ position: moved })
    if (r.ceiling && fallSpeed > 0) fallSpeed = 0
    settle(moved, r.floor)
    if (grounded) {
      fallSpeed = 0
      stopFall?.()
      stopFall = null
    }
  }
  // Airborne: step the fall frame by frame until a floor takes it.
  let fall = () => {
    if (stopFall !== null) return
    let last: number | null = null
    stopFall = onFrame(tick => {
      let now = tick / 1000
      let dt = last === null ? 0 : Math.max(0, Math.min(now - last, MAX_FALL_DT))
      last = now
      fallStep(dt)
    })
  }
  let jump = () => {
    if (!grounded) return
    fallSpeed = JUMP_SPEED
    grounded = false
    fall()
  }

  let lastHud = 0
  onFrame(tick => {
    if (tick - lastHud < HUD_MS) return
    lastHud = tick
    let p = camera.pose().position
    setHud(
      `${pointerLocked() ? "locked - Escape releases" : "click to lock"}  WASD walk, Space jump  x ${p[0].toFixed(1)} y ${(p[1] - EYE).toFixed(2)} z ${p[2].toFixed(1)}  ${grounded ? "grounded" : "airborne"}  pickups ${near().size}`,
    )
  })
  registerDebug("pose", (args?: Record<string, unknown>) => {
    if (args) camera.set(args as Parameters<typeof camera.set>[0])
    return camera.pose()
  })
  registerDebug("walk", (args?: { dx?: number; dz?: number; steps?: number }) => {
    // Frame-sized steps by default: a level's ramps and snap distances
    // are tuned for them, not for one long stride.
    let steps = args?.steps ?? Math.max(1, Math.ceil(Math.hypot(args?.dx ?? 0, args?.dz ?? 0) / WALK_STEP))
    let last: MoveResult | null = null
    for (let i = 0; i < steps; i++) {
      let eye = camera.pose().position
      let r = moveAndSlide(scene, bodyAt(eye), [(args?.dx ?? 0) / steps, 0, (args?.dz ?? 0) / steps], { layers: COLLIDER })
      let moved: Vec3 = [eye[0] + r.motion[0], eye[1] + r.motion[1], eye[2] + r.motion[2]]
      settle(moved, r.floor)
      camera.set({ position: moved })
      last = r
    }
    if (!grounded) fall()
    let p = camera.pose().position
    return { position: p, feet: p[1] - EYE, grounded, wall: last?.wall, ceiling: last?.ceiling, hits: last?.hits.length, pickups: [...nearNow].sort() }
  })
  registerDebug("jump", () => {
    jump()
    return { grounded, fallSpeed }
  })
  registerDebug("fall", (args?: { dt?: number }) => {
    if (!grounded) fallStep(args?.dt ?? 1 / 60)
    return { grounded, fallSpeed, position: camera.pose().position }
  })
  registerDebug("state", () => ({ grounded, fallSpeed, pickups: [...nearNow].sort() }))

  // The ramp: a thin box whose top face runs from the floor at z = +run/2
  // up to the platform's top at z = -run/2, so its rotation about x is the
  // slope and its center sits half a thickness under the top face.
  let slope = Math.atan2(PLATFORM_TOP, RAMP_RUN)
  let rampLength = Math.hypot(RAMP_RUN, PLATFORM_TOP)
  let rampCenter: Vec3 = [0, PLATFORM_TOP / 2 - Math.cos(slope) * (RAMP_THICKNESS / 2), -PLATFORM_HALF - RAMP_RUN / 2 - Math.sin(slope) * (RAMP_THICKNESS / 2)]
  let walls = [
    { position: [0, WALL_HEIGHT / 2, -FLOOR - 0.5] as Vec3, size: [FLOOR * 2 + 2, WALL_HEIGHT, 1] as Vec3 },
    { position: [0, WALL_HEIGHT / 2, FLOOR + 0.5] as Vec3, size: [FLOOR * 2 + 2, WALL_HEIGHT, 1] as Vec3 },
    { position: [-FLOOR - 0.5, WALL_HEIGHT / 2, 0] as Vec3, size: [1, WALL_HEIGHT, FLOOR * 2] as Vec3 },
    { position: [FLOOR + 0.5, WALL_HEIGHT / 2, 0] as Vec3, size: [1, WALL_HEIGHT, FLOOR * 2] as Vec3 },
  ]
  let pillars: Vec3[] = [
    [6, WALL_HEIGHT / 2, 4],
    [-6, WALL_HEIGHT / 2, 4],
    [8, WALL_HEIGHT / 2, -8],
  ]
  let gems: Vec3[] = [
    [3, 0.4, 6],
    [-3, 0.4, 2],
    [7, 0.4, -2],
    [-8, 0.4, -6],
    [0, PLATFORM_TOP + 0.4, -PLATFORM_HALF - RAMP_RUN - PLATFORM_HALF],
  ]

  return (
    <window
      onKeyDown={e => {
        if (e.key === "Escape") lockPointer(false)
        if (e.key === " " || e.code === "Space") jump()
      }}
    >
      <view
        width={pct(100)}
        height={pct(100)}
        onPointerDown={(e: PointerEvent) => {
          if (e.pointerType === "mouse") lockPointer(true)
        }}
      >
        <Scene ref={s => (scene = s)} clearColor={[0.6, 0.72, 0.88, 1]} samples={4} label="collision">
          <PerspectiveCamera fov={70} near={0.1} far={80} />
          <FirstPersonCamera ref={c => (camera = c)} position={[0, EYE, 10]} clampPosition={clamp} />
          <HemisphereLight sky={[0.5, 0.56, 0.68]} ground={[0.25, 0.22, 0.18]} />
          <DirectionalLight color={[1, 0.95, 0.85]} intensity={0.9} position={[8, 14, 6]} direction={[-8, -14, -6]} castShadow shadow={{ mapSize: 2048, normalBias: 0.02, cascades: 3, distance: 40 }} />
          <Mesh geometry={plane({ width: FLOOR * 2 + 2, height: FLOOR * 2 + 2 })} material={ground} rotation={[-Math.PI / 2, 0, 0]} layers={1 | COLLIDER} />
          {walls.map(w => (
            <Mesh geometry={box({ width: w.size[0], height: w.size[1], depth: w.size[2] })} material={brick} position={w.position} layers={1 | COLLIDER} castShadow />
          ))}
          <Mesh
            geometry={box({ width: PLATFORM_HALF * 2, height: PLATFORM_TOP, depth: PLATFORM_HALF * 2 })}
            material={stone}
            position={[0, PLATFORM_TOP / 2, -PLATFORM_HALF - RAMP_RUN - PLATFORM_HALF]}
            layers={1 | COLLIDER}
            castShadow
          />
          <Mesh geometry={box({ width: 2.5, height: RAMP_THICKNESS, depth: rampLength })} material={stone} position={rampCenter} rotation={[slope, 0, 0]} layers={1 | COLLIDER} castShadow />
          {pillars.map(p => (
            <Mesh geometry={cylinder({ radiusTop: 0.35, radiusBottom: 0.4, height: WALL_HEIGHT })} material={stone} position={p} layers={1 | COLLIDER} castShadow />
          ))}
          {gems.map((g, i) => (
            <Mesh ref={m => (pickups[i] = m)} geometry={sphere({ radius: 0.25 })} material={near().has(i) ? bright : dim} position={g} castShadow />
          ))}
        </Scene>
        <text position="absolute" left={12} top={12} color="#ffffff" fontSize={14}>
          {hud()}
        </text>
      </view>
    </window>
  )
}

render(() => <App />)
