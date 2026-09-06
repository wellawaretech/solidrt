// A first-person walk: `<FirstPersonCamera>` driving the scene camera from
// WASD/arrows and the gamepad sticks, mouse look under pointer lock and
// drag-to-look without it. Click the scene to lock the pointer (the scene
// leaf takes focus on the same click, which routes the keys to the
// control), Escape releases it; on a touch screen a drag looks around and
// the left stick of a pad walks. Movement runs a frame loop only while a
// key is held or a stick deflected - a still scene renders nothing new.
// `clampPosition` keeps the walker inside the courtyard walls, the whole
// of the collision an app gets from a camera control. The `pose` debug
// command reads and sets the pose for headless checks.
import { createSignal, lockPointer, onFrame, pct, pointerLocked, render } from "@solidrt/core"
import type { PointerEvent } from "@solidrt/core"
import { box, cylinder, DirectionalLight, FirstPersonCamera, HemisphereLight, lit, Mesh, PerspectiveCamera, plane, Scene } from "@solidrt/3d"
import type { FirstPersonCameraHandle, Vec3 } from "@solidrt/3d"
import { registerDebug } from "srt:dev"

// Half extent of the walkable courtyard in world units; the walls stand
// just outside it.
const COURT = 9
// How far inside the walls the eye may go.
const WALL_GAP = 0.6
// Standing eye height.
const EYE = 1.6
// Pillar ring radius and count.
const RING = 5
const PILLARS = 8
// Half extent of the sun's shadow box: it must cover the far courtyard
// corners as seen from the light, or their shadows clip (anything outside
// the box is lit). The corner tops sit ~13 units off the light's axis.
const SHADOW_EXTENT = 16
// HUD refresh interval in ms: the pose readout is slow UI state, not a
// per-frame binding.
const HUD_MS = 100

let clampNum = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))
let inside = (p: Vec3): Vec3 => [clampNum(p[0], -COURT + WALL_GAP, COURT - WALL_GAP), EYE, clampNum(p[2], -COURT + WALL_GAP, COURT - WALL_GAP)]

function App() {
  let camera!: FirstPersonCameraHandle
  let [hud, setHud] = createSignal("")
  let lastHud = 0
  onFrame(tick => {
    if (tick - lastHud < HUD_MS) return
    lastHud = tick
    let p = camera.pose()
    setHud(
      `${pointerLocked() ? "locked - Escape releases" : "click to lock"}  x ${p.position[0].toFixed(1)} z ${p.position[2].toFixed(1)}  yaw ${p.yaw.toFixed(2)} pitch ${p.pitch.toFixed(2)}`,
    )
  })
  registerDebug("pose", (args?: Record<string, unknown>) => {
    if (args) camera.set(args as Parameters<typeof camera.set>[0])
    return camera.pose()
  })

  let ground = lit({ color: [0.5, 0.52, 0.48] })
  let stone = lit({ color: [0.75, 0.72, 0.66] })
  let brick = lit({ color: [0.6, 0.32, 0.26] })
  let gold = lit({ color: [0.9, 0.75, 0.3], specular: 0.5, shininess: 40 })

  let pillars = Array.from({ length: PILLARS }, (_, i) => {
    let a = (i / PILLARS) * Math.PI * 2
    return [Math.sin(a) * RING, Math.cos(a) * RING] as const
  })
  let walls = [
    { position: [0, 1.5, -COURT - 0.5] as Vec3, size: [COURT * 2 + 2, 3, 1] as Vec3 },
    { position: [0, 1.5, COURT + 0.5] as Vec3, size: [COURT * 2 + 2, 3, 1] as Vec3 },
    { position: [-COURT - 0.5, 1.5, 0] as Vec3, size: [1, 3, COURT * 2] as Vec3 },
    { position: [COURT + 0.5, 1.5, 0] as Vec3, size: [1, 3, COURT * 2] as Vec3 },
  ]

  return (
    <window
      onKeyDown={e => {
        if (e.key === "Escape") lockPointer(false)
      }}
    >
      <view
        width={pct(100)}
        height={pct(100)}
        onPointerDown={(e: PointerEvent) => {
          if (e.pointerType === "mouse") lockPointer(true)
        }}
      >
        <Scene clearColor={[0.6, 0.72, 0.88, 1]} samples={4} label="first-person">
          <PerspectiveCamera fov={70} near={0.1} far={60} />
          <FirstPersonCamera ref={c => (camera = c)} position={[0, EYE, 7]} clampPosition={inside} />
          <HemisphereLight sky={[0.5, 0.56, 0.68]} ground={[0.25, 0.22, 0.18]} />
          <DirectionalLight
            color={[1, 0.95, 0.85]}
            intensity={0.9}
            position={[6, 10, 4]}
            direction={[-6, -10, -4]}
            castShadow
            shadow={{ mapSize: 2048, normalBias: 0.02, camera: { near: 1, far: 40, left: -SHADOW_EXTENT, right: SHADOW_EXTENT, top: SHADOW_EXTENT, bottom: -SHADOW_EXTENT } }}
          />
          <Mesh geometry={plane({ width: COURT * 2 + 2, height: COURT * 2 + 2 })} material={ground} rotation={[-Math.PI / 2, 0, 0]} />
          {walls.map(w => (
            <Mesh geometry={box({ width: w.size[0], height: w.size[1], depth: w.size[2] })} material={brick} position={w.position} castShadow />
          ))}
          {pillars.map(([x, z]) => (
            <Mesh geometry={cylinder({ radiusTop: 0.3, radiusBottom: 0.35, height: 3 })} material={stone} position={[x, 1.5, z]} castShadow />
          ))}
          <Mesh geometry={box({ width: 1.2, height: 1.2, depth: 1.2 })} material={gold} position={[0, 0.6, 0]} castShadow />
        </Scene>
        <text position="absolute" left={12} top={12} color="#ffffff" fontSize={14}>
          {hud()}
        </text>
      </view>
    </window>
  )
}

render(() => <App />)
