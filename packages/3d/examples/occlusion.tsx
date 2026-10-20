// Occlusion culling: a walled courtyard whose walls are occluders
// (`occluder="geometry"` on the wall meshes, the exact shape: a triangle
// occludes what lies behind it) around a crowd of solids, and the same
// crowd again outside. Orbit the camera: from outside, the solids the
// walls cover are not drawn - the spatial core rasterizes the shown
// occluders into a small depth buffer per target when the camera or an
// occluder moves and switches off every mesh whose box lies wholly
// behind it, no per-frame JS - and through the gate they come back. The
// gate's door is an occluder too: a hidden occluder occludes nothing, so
// toggling it open uncovers the courtyard at once. The HUD counts what
// the scene's render currently leaves out, read back with
// scene.cullState per mesh.
//
// Debug: `door { open }` opens or closes the gate (a hidden mesh), `park
// { azimuth }` holds the orbit at an azimuth so a snapshot is
// deterministic, `state` reports the counts.
import { createInputMap, createPointerFeed, createSignal, gamepad, onFrame, pct, render } from "@solidrt/core"
import { box, DirectionalLight, HemisphereLight, icosahedron, Mesh, OrbitCamera, orbitActions, orbitBindings, phong, plane, Scene, setVisible, torusKnot } from "@solidrt/3d"
import type { MeshNode, SceneHandle } from "@solidrt/3d"
import { registerDebug } from "sol:dev"

// The courtyard: a square of this half side, walls this high and thick,
// the gate this wide in the south wall.
const COURT = 10
const WALL_HEIGHT = 5
const WALL_THICKNESS = 0.6
const GATE = 4
// Solids per side of the inner and the outer crowd.
const INNER = 6
const OUTER = 10
// Frames between HUD recounts (a cullState read per mesh, cheap, but a
// count every frame is not what the HUD is for).
const RECOUNT_FRAMES = 15

let scene: SceneHandle | undefined
let door: MeshNode | undefined
let solids: MeshNode[] = []
let parked: number | null = null
let [occluded, setOccluded] = createSignal(0)
registerDebug("door", (args?: Record<string, unknown>) => {
  if (door !== undefined) setVisible(door, args?.open !== true)
  return { open: door === undefined ? null : !door.visible }
})
registerDebug("park", (args?: Record<string, unknown>) => {
  parked = typeof args?.azimuth === "number" ? args.azimuth : null
  return { parked }
})
registerDebug("state", () => ({ occluded: occluded(), solids: solids.length, doorOpen: door === undefined ? null : !door.visible }))

function App() {
  let pointer = createPointerFeed()
  let input = createInputMap(orbitActions)
  input.bind(orbitBindings({ pointer, gamepad: gamepad() }))
  let brick = phong({ color: [0.6, 0.35, 0.25] })
  let stone = phong({ color: [0.55, 0.6, 0.65] })
  let brass = phong({ color: [0.8, 0.65, 0.3], shininess: 60 })
  let knot = torusKnot({ radius: 0.5, tube: 0.18 })
  let ball = icosahedron({ radius: 0.6, detail: 1 })
  // The south wall is two pieces with the gate between them; the door
  // fills the gate (an occluder while shown).
  let sideWall = box({ width: COURT * 2 + WALL_THICKNESS, height: WALL_HEIGHT, depth: WALL_THICKNESS })
  let gatePiece = box({ width: COURT - GATE / 2, height: WALL_HEIGHT, depth: WALL_THICKNESS })
  let doorLeaf = box({ width: GATE, height: WALL_HEIGHT, depth: WALL_THICKNESS / 2 })
  let crowd = (side: number, spread: number, geometry: ReturnType<typeof box>, material: ReturnType<typeof phong>, y: number) =>
    Array.from({ length: side * side }, (_, i) => {
      let x = ((i % side) / (side - 1) - 0.5) * spread * 2
      let z = (Math.floor(i / side) / (side - 1) - 0.5) * spread * 2
      return <Mesh geometry={geometry} material={material} position={[x, y, z]} ref={m => solids.push(m)} />
    })
  let azimuth = 0.7
  onFrame((tick, frame) => {
    if (frame % RECOUNT_FRAMES === 0 && scene !== undefined) {
      let n = 0
      for (let m of solids) if (scene.cullState(m) === "occluded") n++
      setOccluded(n)
    }
    azimuth = parked ?? tick / 9000
  })
  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.62, 0.74, 0.9, 1]} label="occlusion" ref={s => (scene = s)} pointer={pointer}>
          <OrbitCamera input={input} target={[0, 2, 0]} distance={30} elevation={0.25} azimuth={azimuth} />
          <HemisphereLight sky={[0.7, 0.8, 1]} ground={[0.3, 0.28, 0.22]} intensity={0.9} />
          <DirectionalLight direction={[-0.5, -1, -0.3]} intensity={1.1} />
          <Mesh geometry={plane({ width: 80, height: 80 })} material={phong({ color: [0.36, 0.42, 0.24] })} rotation={[-Math.PI / 2, 0, 0]} />
          <Mesh geometry={sideWall} material={brick} position={[0, WALL_HEIGHT / 2, -COURT]} occluder="geometry" />
          <Mesh geometry={sideWall} material={brick} position={[-COURT, WALL_HEIGHT / 2, 0]} rotation={[0, Math.PI / 2, 0]} occluder="geometry" />
          <Mesh geometry={sideWall} material={brick} position={[COURT, WALL_HEIGHT / 2, 0]} rotation={[0, Math.PI / 2, 0]} occluder="geometry" />
          <Mesh geometry={gatePiece} material={brick} position={[-(COURT + GATE / 2) / 2, WALL_HEIGHT / 2, COURT]} occluder="geometry" />
          <Mesh geometry={gatePiece} material={brick} position={[(COURT + GATE / 2) / 2, WALL_HEIGHT / 2, COURT]} occluder="geometry" />
          <Mesh geometry={doorLeaf} material={brass} position={[0, WALL_HEIGHT / 2, COURT]} occluder="geometry" ref={m => (door = m)} />
          {crowd(INNER, COURT - 2, knot, brass, 1)}
          {crowd(OUTER, COURT * 2.4, ball, stone, 0.6)}
        </Scene>
        <text position="absolute" left={16} top={16} fontSize={18} color="#ffffff">
          {"occluded " + occluded() + " of " + solids.length + " solids"}
        </text>
      </view>
    </window>
  )
}

render(() => <App />)
