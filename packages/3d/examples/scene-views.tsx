// One scene, three renderings: the built-in leaf from a perspective
// camera, plus two `scene.createView` targets - a top-down ORTHOGRAPHIC
// map (`ortho` on setCamera) and a side silhouette drawn with an
// `overrideMaterial`. The group spins from ONE signal: each mesh is one
// core node whose flush writes its world matrix into every target's entry
// at once, so the extra views cost the app nothing per frame (and the
// light set fans out to them too - the map is lit like the main view).
// Views die with the scene.
//
// Layers and per-view fog: the marker disc is on layer bit 2, so the
// main render (mask 1) and the silhouette never draw it - only the map,
// whose mask admits both bits. The scene is fogged; `fog: null` on the
// map keeps the top-down view clear (its fog names are view-owned, so
// scene-wide fog writes never clobber them).
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { box, circle, DirectionalLight, Group, HemisphereLight, lit, Mesh, PerspectiveCamera, plane, Scene, sphere, unlit } from "@solidrt/3d"
import type { SceneHandle } from "@solidrt/3d"

// Layer bits: the world everything defaults to, and the map-only markers.
const WORLD_LAYER = 1
const MARKER_LAYER = 2

const MAIN = 720
const SIDE = 360

function App() {
  let [spin, setSpin] = createSignal(0)
  onFrame(tick => setSpin(tick / 2000))

  let cube = box()
  let floor = plane({ width: 6, height: 6, label: "floor" })
  let ball = sphere({ radius: 0.35 })
  let marker = circle({ radius: 0.3, label: "marker" })
  // `ref` runs before `output`, so the handle is set when the views are
  // created there; both callbacks run once, untracked - no signals needed.
  let scene!: SceneHandle

  return (
    <window>
      <view width={pct(100)} height={pct(100)} designSize={[MAIN + SIDE, MAIN]}>
        <Scene
          width={MAIN}
          height={MAIN}
          clearColor={[0.07, 0.07, 0.1, 1]}
          fog={{ color: [0.07, 0.07, 0.1], near: 2.5, far: 9 }}
          label="scene-views"
          ref={s => (scene = s)}
          output={tex => {
            // The map: straight down from y = 10, world -z toward the top of
            // the leaf, seven world units across at any depth. Its mask also
            // admits the marker layer, and `fog: null` keeps it clear while
            // the main view fades to the horizon.
            let map = scene.createView({
              width: SIDE,
              height: SIDE,
              clearColor: [0.05, 0.08, 0.06, 1],
              layers: WORLD_LAYER | MARKER_LAYER,
              fog: null,
              label: "map",
            })
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
            {/* The ball's map marker: drawn by the map view alone. */}
            <Mesh
              geometry={marker}
              material={unlit({ color: [0.95, 0.25, 0.2] })}
              layers={MARKER_LAYER}
              position={[1.1, 2, 0]}
              rotation={[-Math.PI / 2, 0, 0]}
            />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
