// One scene, three renderings: the built-in leaf from a perspective
// camera, plus two `scene.createView` targets - a top-down ORTHOGRAPHIC
// map (`ortho` on setCamera) and a side silhouette drawn with an
// `overrideMaterial`. The group spins from ONE signal: each mesh is one
// core node whose flush writes its world matrix into every target's entry
// at once, so the extra views cost the app nothing per frame (and the
// light set fans out to them too - the map is lit like the main view).
// Views die with the scene.
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { box, DirectionalLight, Group, HemisphereLight, lit, Mesh, PerspectiveCamera, plane, Scene, sphere, unlit } from "@solidrt/3d"
import type { SceneHandle } from "@solidrt/3d"

const MAIN = 720
const SIDE = 360

function App() {
  let [spin, setSpin] = createSignal(0)
  onFrame(tick => setSpin(tick / 2000))

  let cube = box()
  let floor = plane({ width: 6, height: 6, label: "floor" })
  let ball = sphere({ radius: 0.35 })
  // `ref` runs before `output`, so the handle is set when the views are
  // created there; both callbacks run once, untracked - no signals needed.
  let scene!: SceneHandle

  return (
    <window>
      <view width={pct(100)} height={pct(100)} viewBox={[MAIN + SIDE, MAIN]}>
        <Scene
          width={MAIN}
          height={MAIN}
          clearColor={[0.07, 0.07, 0.1, 1]}
          label="split-screen"
          ref={s => (scene = s)}
          output={tex => {
            // The map: straight down from y = 10, world -z toward the top of
            // the leaf, seven world units across at any depth.
            let map = scene.createView({ width: SIDE, height: SIDE, clearColor: [0.05, 0.08, 0.06, 1], label: "map" })
            map.setCamera({
              position: [0, 10, 0],
              target: [0, 0, 0],
              up: [0, 0, -1],
              ortho: { left: -3.5, right: 3.5, top: 3.5, bottom: -3.5 },
              near: 0.1,
              far: 20,
            })
            // The silhouette: every mesh drawn flat amber, from the side. The
            // floor is edge-on here, a single line at the bottom.
            let side = scene.createView({
              width: SIDE,
              height: SIDE,
              clearColor: [0.1, 0.06, 0.05, 1],
              overrideMaterial: unlit({ color: [0.95, 0.75, 0.3] }),
              label: "silhouette",
            })
            side.setCamera({
              position: [6, 1, 0],
              target: [0, 0.6, 0],
              ortho: { left: -2.4, right: 2.4, top: 2.4, bottom: -2.4 },
              near: 0.1,
              far: 20,
            })
            return (
              <view flexDirection="row" width={MAIN + SIDE} height={MAIN}>
                <texture src={tex} width={MAIN} height={MAIN} />
                <view flexDirection="column" width={SIDE} height={MAIN}>
                  <texture src={map.texture} width={SIDE} height={SIDE} />
                  <texture src={side.texture} width={SIDE} height={SIDE} />
                </view>
              </view>
            )
          }}
        >
          <PerspectiveCamera fov={55} position={[0, 1.6, 3.6]} lookAt={[0, 0.3, 0]} />
          <HemisphereLight sky={[0.6, 0.7, 0.9]} ground={[0.25, 0.2, 0.15]} intensity={0.6} />
          <DirectionalLight direction={[-0.5, -1, -0.3]} color={[1, 0.95, 0.85]} intensity={0.9} />
          <Mesh geometry={floor} material={lit({ color: [0.3, 0.32, 0.38] })} rotation={[-Math.PI / 2, 0, 0]} />
          <Group rotation={[0, spin(), 0]}>
            <Mesh geometry={cube} material={lit({ color: [0.85, 0.3, 0.3] })} position={[0, 0.5, 0]} />
            <Mesh
              geometry={cube}
              material={lit({ color: [0.9, 0.8, 0.35] })}
              position={[-1.1, 0.7, 0]}
              scale={[0.5, 1.4, 0.5]}
            />
            <Mesh geometry={ball} material={lit({ color: [0.35, 0.65, 0.9] })} position={[1.1, 0.35, 0]} />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
