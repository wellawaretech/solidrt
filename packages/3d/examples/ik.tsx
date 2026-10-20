// Inverse kinematics: one meshless biped with the four solver kinds
// stacked on it by createIk and solved in the spatial core, after each
// frame's JS and before any spring bones. The only per-frame JS moves
// the three targets; every joint the solvers own is bent natively.
//   - lookAt   spine, neck and head share the aim at the red ball; yaw
//              and pitch are limited, so when the ball passes behind the
//              head eases through rest instead of snapping across
//   - twoBone  the arm reaches the green handle, a pole keeping the
//              elbow out, a soft band near full extension
//   - chain    the tail (FABRIK, a magnet above) chases the yellow point
//   - feet     both feet plant on the sloped floor from ground rays the
//              core casts on the floor's collider layer, the hips dropping
//              so the lower foot reaches; declared FIRST, so the arm and
//              the tail solve under the moved hips
import { onFrame, pct, render } from "@solidrt/core"
import { add, box, capsule, createGroup, createIk, createMesh, createModel, DirectionalLight, HemisphereLight, PerspectiveCamera, Scene, setLayers, setTransform, sphere, standard } from "@solidrt/3d"
import type { Model, ModelData, ModelNode, SceneNode } from "@solidrt/3d"

const SIZE = 720
/** The collider layer the ground rays see; the floor draws on layer 1
 * as well. */
const GROUND_LAYER = 2
/** The floor's tilt about z, radians. */
const SLOPE = 0.35
/** The targets' orbit periods, ms. */
const HEAD_PERIOD_MS = 6000
const HAND_PERIOD_MS = 2500
const TAIL_PERIOD_MS = 4000
const QUARTER_X: [number, number, number, number] = [Math.sin(-Math.PI / 4), 0, 0, Math.cos(Math.PI / 4)]

function rigData(): ModelData {
  let nodes: ModelNode[] = []
  let at = (name: string, parent: number | null, position: [number, number, number]) => {
    nodes.push({ name, parent, position, rotation: [0, 0, 0, 1], scale: [1, 1, 1] })
    return nodes.length - 1
  }
  let root = at("root", null, [0, 0, 0])
  let hips = at("hips", root, [0, 1, 0])
  let spine = at("spine", hips, [0, 0.3, 0])
  let neck = at("neck", spine, [0, 0.3, 0])
  at("head", neck, [0, 0.15, 0])
  let shoulder = at("shoulder", neck, [0.3, -0.05, 0])
  let elbow = at("elbow", shoulder, [0, -0.3, 0])
  at("wrist", elbow, [0, -0.3, 0])
  for (let side of ["l", "r"]) {
    let hip = at(side + "hip", hips, [side === "l" ? -0.2 : 0.2, 0, 0])
    let knee = at(side + "knee", hip, [0, -0.5, 0])
    at(side + "ankle", knee, [0, -0.5, 0])
  }
  let tail = at("tail0", hips, [0, 0, -0.1])
  for (let i = 1; i < 5; i++) tail = at("tail" + i, tail, [0, 0, -0.15])
  return { nodes, parts: [], skins: [], clips: [], materials: [], images: [], bounds: new Float32Array([-1, -1, -1, 1, 2, 1]) }
}

function App() {
  let model: Model = createModel(rigData(), { label: "rig" })
  let j = (name: string): SceneNode => model.nodes.find((n) => n.name === name)!.node

  let bone = standard({ color: [0.85, 0.6, 0.3] })
  let mark = standard({ color: [0.3, 0.6, 0.85] })
  for (let { name, node } of model.nodes) {
    if (name === "root") continue
    add(node, createMesh(sphere({ radius: name === "head" ? 0.1 : 0.035 }), name === "head" ? mark : bone))
  }
  let nose = createMesh(sphere({ radius: 0.03 }), bone)
  setTransform(nose, { position: [0, 0, 0.1] })
  add(j("head"), nose)
  for (let name of ["shoulder", "elbow", "lhip", "lknee", "rhip", "rknee"]) {
    let len = name.endsWith("hip") || name.endsWith("knee") ? 0.5 : 0.3
    let mesh = createMesh(capsule({ radius: 0.025, height: len - 0.05 }), bone)
    setTransform(mesh, { position: [0, -len / 2, 0] })
    add(j(name), mesh)
  }
  for (let name of ["hips", "spine"]) {
    let mesh = createMesh(capsule({ radius: 0.03, height: 0.25 }), bone)
    setTransform(mesh, { position: [0, 0.15, 0] })
    add(j(name), mesh)
  }
  for (let i = 0; i < 4; i++) {
    let mesh = createMesh(capsule({ radius: 0.02, height: 0.1 }), bone)
    setTransform(mesh, { position: [0, 0, -0.075], quaternion: QUARTER_X })
    add(j("tail" + i), mesh)
  }

  let ball = createMesh(sphere({ radius: 0.06 }), standard({ color: [0.9, 0.2, 0.2] }))
  let handle = createMesh(sphere({ radius: 0.04 }), standard({ color: [0.2, 0.9, 0.3] }))
  let tailTarget = createMesh(sphere({ radius: 0.04 }), standard({ color: [0.9, 0.8, 0.2] }))
  let floor = createMesh(box({ width: 4, height: 0.2, depth: 4 }), standard({ color: [0.4, 0.4, 0.45] }))
  setTransform(floor, { position: [0, -0.1, 0], quaternion: [0, 0, Math.sin(SLOPE / 2), Math.cos(SLOPE / 2)] })
  setLayers(floor, 1 | GROUND_LAYER)

  onFrame((tick) => {
    let a = (tick / HEAD_PERIOD_MS) * 2 * Math.PI
    setTransform(ball, { position: [Math.sin(a) * 1.5, 1.7 + Math.sin(a * 0.7) * 0.4, Math.cos(a) * 1.5] })
    let b = (tick / HAND_PERIOD_MS) * 2 * Math.PI
    setTransform(handle, { position: [0.4 + Math.cos(b) * 0.15, 1.15 + Math.sin(b) * 0.15, 0.25] })
    let c = (tick / TAIL_PERIOD_MS) * 2 * Math.PI
    setTransform(tailTarget, { position: [Math.sin(c) * 0.35, 0.85 + Math.cos(c) * 0.25, -0.55] })
  })

  return (
    <window title="ik">
      <view width={pct(100)} height={pct(100)} designSize={[SIZE, SIZE]}>
        <Scene
          width={SIZE}
          height={SIZE}
          clearColor={[0.08, 0.08, 0.1, 1]}
          samples={4}
          label="ik"
          ref={(scene) => {
            let world = createGroup()
            add(scene.root, world)
            add(world, model)
            for (let n of [ball, handle, tailTarget, floor]) add(world, n)
            createIk(model, {
              solvers: [
                { kind: "feet", hips: j("hips"), legs: [{ root: j("lhip"), mid: j("lknee"), tip: j("lankle") }, { root: j("rhip"), mid: j("rknee"), tip: j("rankle") }], layers: GROUND_LAYER },
                {
                  kind: "lookAt",
                  bones: [{ node: j("spine"), share: 0.2 }, { node: j("neck"), share: 0.3 }, { node: j("head"), share: 0.5 }],
                  target: { node: ball },
                  limit: { yaw: 1.2, pitch: 0.8, damp: 0.7 },
                },
                { kind: "twoBone", root: j("shoulder"), mid: j("elbow"), tip: j("wrist"), target: { node: handle }, pole: { point: [0.6, 1.2, -0.5] }, soft: 0.15 },
                { kind: "chain", root: j("tail0"), tip: j("tail4"), target: { node: tailTarget }, magnet: { point: [0, 2, -0.6] } },
              ],
            })
          }}
        >
          <PerspectiveCamera fov={40} position={[1.2, 1.6, 3.4]} lookAt={[0, 1, 0]} />
          <HemisphereLight sky={[1, 1, 1]} ground={[0.3, 0.3, 0.35]} intensity={0.6} />
          <DirectionalLight position={[4, 12, 8]} intensity={1.4} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
