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
import { add, DirectionalLight, Group, HemisphereLight, loadModel, PerspectiveCamera, Scene } from "@solidrt/3d"
import type { Model, SceneNode } from "@solidrt/3d"

const SIZE = 720

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
    let b = model.bounds
    let center: [number, number, number] = [(b[0]! + b[3]!) / 2, (b[1]! + b[4]!) / 2, (b[2]! + b[5]!) / 2]
    let radius = Math.hypot(b[3]! - b[0]!, b[4]! - b[1]!, b[5]! - b[2]!) / 2
    return (
      <Scene width={SIZE} height={SIZE} clearColor={[0.1, 0.11, 0.14, 1]} samples={4} label="model-load">
        <PerspectiveCamera fov={40} position={[center[0] + radius * 1.4, center[1] + radius * 1.1, center[2] + radius * 2.2]} lookAt={center} />
        <HemisphereLight sky={[0.45, 0.5, 0.6]} ground={[0.2, 0.17, 0.14]} />
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
      <view width={pct(100)} height={pct(100)} designSize={[SIZE, SIZE]}>
        <Loading fallback={<text fontSize={24} color="#8899aa">Loading rover...</text>}>
          <Rover turn={() => t() / 3} />
        </Loading>
      </view>
    </window>
  )
}

render(() => <App />)
