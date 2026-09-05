// A model from a file: model.glb (a small authored scene - a rover with a
// node tree of nested transforms, a mirrored node, a textured material, a
// transparent dome and a mesh shipped without normals) parsed at runtime
// with parseGltf and turned into a Group of meshes by createModel. The
// glb rides in the bundle as a binary import; a model under assets/ goes
// through loadGltf (same parser, read with flux:fs) or, baked once with
// `srt tool 3d/model`, through loadModel (no parsing at all). Every part
// keeps its node name and is an ordinary mesh: pointer events, transforms
// and materials work per part. The materials are createModel's default
// `standard` from the file's PBR factors, so the scene carries an
// environment: the steel reflects it and every part takes its diffuse
// ambient from it (a metal with nothing to reflect renders near black) -
// here a sky-to-ground gradient built at startup, in an app usually a
// baked HDRI (see environment.tsx).

import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { createTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { add, createModel, DirectionalLight, equirectToCube, Group, parseGltf, PerspectiveCamera, Scene, setVisible } from "@solidrt/3d"
import type { SceneNode } from "@solidrt/3d"
import modelBytes from "./model.glb" with { type: "binary" }

// Face edge of the gradient environment cube: a gradient needs no detail.
const ENV_FACE = 16
// The gradient, zenith to nadir, sRGB bytes: sky, horizon, ground, below.
const ENV_ROWS: [number, number, number][] = [
  [96, 122, 168],
  [176, 190, 210],
  [120, 105, 90],
  [60, 52, 45],
]

// A tiny equirectangular panorama (2 columns, one row per band, linear
// filtering between) turned into the cube the scene lights by.
function gradientEnvironment(): TextureId {
  let px = new Uint8Array(2 * ENV_ROWS.length * 4)
  ENV_ROWS.forEach((c, y) => {
    for (let x = 0; x < 2; x++) px.set([c[0], c[1], c[2], 255], (y * 2 + x) * 4)
  })
  let panorama = createTexture(px, 2, ENV_ROWS.length, { format: "rgba8-srgb", filter: "linear" })
  return equirectToCube(panorama, ENV_FACE, { format: "rgba8-srgb", mipmap: true, label: "gradient-env" })
}

function App() {
  let cube = gradientEnvironment()
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
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.1, 0.11, 0.14, 1]} environment={{ cube }} samples={4} label="model">
          <PerspectiveCamera fov={40} position={[center[0] + radius * 1.4, center[1] + radius * 1.1, center[2] + radius * 2.2]} lookAt={center} />
          <DirectionalLight direction={[0.5, -0.8, 0.3]} color={[1, 0.95, 0.85]} intensity={0.9} />
          <Group rotation={[0, t() / 3, 0]} ref={(g: SceneNode) => add(g, model)} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
