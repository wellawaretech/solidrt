// Async model loading: loadModel + <Loading>. The same rover as
// model.tsx, but read from assets/ at runtime instead of riding in the
// bundle - the shape every app with real content ends up needing. Bake
// the asset once and drop it in your app's assets tree:
//
//   bunx srt tool 3d/model examples/model.glb -o assets/model.srtm
//
// (loadGltf("assets/model.glb") has the exact same async shape; the bake
// just removes the runtime parse.)
//
// TWO rules make the async read work, both enforced by the runtime:
//
// 1. Read the async value only inside a TRACKING scope whose result JSX
//    reads back - the memo pattern below. Reading it straight in the
//    component body (`let model = loaded()`) throws
//    PENDING_ASYNC_UNTRACKED_READ: outside tracking there is nothing to
//    suspend and retry.
// 2. The component under <Loading> creates NO elements of its own before
//    the suspending read. An element built ahead of a suspend is orphaned
//    when the boundary discards and retries the subtree, and orphaned
//    subtrees are never freed (the dev leak sentinel reports them). So
//    the window/view shell lives in App ABOVE the boundary, and Rover
//    suspends at `loaded()` before building any JSX.

import { createMemo, createSignal, Loading, onFrame, pct, render } from "@solidrt/core"
import { createTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { add, DirectionalLight, equirectToCube, Group, loadModel, PerspectiveCamera, Scene } from "@solidrt/3d"
import type { Model, SceneNode } from "@solidrt/3d"

// The same gradient environment as model.tsx: the default `standard`
// materials want one (a baked HDRI in a real app, see environment.tsx).
const ENV_FACE = 16
const ENV_ROWS: [number, number, number][] = [
  [96, 122, 168],
  [176, 190, 210],
  [120, 105, 90],
  [60, 52, 45],
]

function gradientEnvironment(): TextureId {
  let px = new Uint8Array(2 * ENV_ROWS.length * 4)
  ENV_ROWS.forEach((c, y) => {
    for (let x = 0; x < 2; x++) px.set([c[0], c[1], c[2], 255], (y * 2 + x) * 4)
  })
  let panorama = createTexture(px, 2, ENV_ROWS.length, { format: "rgba8-srgb", filter: "linear" })
  return equirectToCube(panorama, ENV_FACE, { format: "rgba8-srgb", mipmap: true, label: "gradient-env" })
}

function Rover(props: { turn: () => number }) {
  // The async value: a memo returning a Promise. Its read below suspends
  // until the file is read and the model built, then resumes under the
  // <Loading> boundary in App.
  let loaded = createMemo(() => loadModel("assets/model.srtm", { label: "rover" }))

  // Everything derived from the model - framing, mounting - happens in
  // this memo, AFTER the suspending read, so nothing here runs or builds
  // until the model exists. The memo returns the scene JSX; the component
  // returns the memo read and nothing else.
  let scene = createMemo(() => {
    let model: Model = loaded()
    let cube = gradientEnvironment()
    let b = model.bounds
    let center: [number, number, number] = [(b[0]! + b[3]!) / 2, (b[1]! + b[4]!) / 2, (b[2]! + b[5]!) / 2]
    let radius = Math.hypot(b[3]! - b[0]!, b[4]! - b[1]!, b[5]! - b[2]!) / 2
    return (
      <Scene clearColor={[0.1, 0.11, 0.14, 1]} environment={{ cube }} samples={4} label="model-load">
        <PerspectiveCamera fov={40} position={[center[0] + radius * 1.4, center[1] + radius * 1.1, center[2] + radius * 2.2]} lookAt={center} />
        <DirectionalLight direction={[0.5, -0.8, 0.3]} color={[1, 0.95, 0.85]} intensity={0.9} />
        <Group rotation={[0, props.turn(), 0]} ref={(g: SceneNode) => add(g, model)} />
      </Scene>
    )
  })

  return <>{scene()}</>
}

function App() {
  let [t, setT] = createSignal(0)
  onFrame(tick => setT(tick / 1000))

  // The shell above the boundary; the fallback is plain UI and may be
  // anything - it is created and torn down by <Loading> itself.
  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Loading fallback={<text fontSize={24} color="#8899aa">Loading rover...</text>}>
          <Rover turn={() => t() / 3} />
        </Loading>
      </view>
    </window>
  )
}

render(() => <App />)
