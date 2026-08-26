// A model from a file: model.glb (a small authored scene - a rover with a
// node tree of nested transforms, a mirrored node, a textured material, a
// transparent dome and a mesh shipped without normals) parsed at runtime
// with parseGltf and turned into a Group of meshes by createModel. The
// glb rides in the bundle as a binary import; a model under assets/ goes
// through loadGltf (same parser, read with flux:fs) or, baked once with
// `srt tool 3d/model`, through loadModel (no parsing at all). Every part
// keeps its node name and is an ordinary mesh: pointer events, transforms
// and materials work per part.

import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { add, createModel, DirectionalLight, Group, HemisphereLight, parseGltf, PerspectiveCamera, Scene, setVisible } from "@solidrt/3d"
import type { SceneNode } from "@solidrt/3d"
import modelBytes from "./model.glb" with { type: "binary" }

const SIZE = 720

function App() {
  let [t, setT] = createSignal(0)
  onFrame(tick => setT(tick / 1000))

  // Parse + upload once; the model is plain scene data from here on.
  let model = createModel(parseGltf(modelBytes), { label: "rover" })
  let b = model.bounds
  let center: [number, number, number] = [(b[0]! + b[3]!) / 2, (b[1]! + b[4]!) / 2, (b[2]! + b[5]!) / 2]
  let radius = Math.hypot(b[3]! - b[0]!, b[4]! - b[1]!, b[5]! - b[2]!) / 2

  // Parts are named after the glTF nodes: a click hides that part, a
  // click on the body brings everything back.
  for (let part of model.parts) {
    part.mesh.onPointerDown = () => {
      if (part.name === "body") for (let p of model.parts) setVisible(p.mesh, true)
      else setVisible(part.mesh, false)
    }
  }

  return (
    <window>
      <view width={pct(100)} height={pct(100)} viewBox={[SIZE, SIZE]}>
        <Scene width={SIZE} height={SIZE} clearColor={[0.1, 0.11, 0.14, 1]} samples={4} label="model">
          <PerspectiveCamera fov={40} position={[center[0] + radius * 1.4, center[1] + radius * 1.1, center[2] + radius * 2.2]} lookAt={center} />
          <HemisphereLight sky={[0.45, 0.5, 0.6]} ground={[0.2, 0.17, 0.14]} />
          <DirectionalLight direction={[0.5, -0.8, 0.3]} color={[1, 0.95, 0.85]} intensity={0.9} />
          <Group rotation={[0, t() / 3, 0]} ref={(g: SceneNode) => add(g, model)} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
