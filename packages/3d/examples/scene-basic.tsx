// A minimal scene: unlit meshes with real cross-mesh occlusion (one shared
// depth buffer), a spinning group, and a fixed camera. One onFrame loop
// drives one signal; the library keeps every draw entry's uModel (and the
// scene target's shared uViewProj) in step.
// The sphere orbits with the group, crossing behind and in front of the
// tall box - that alternation is the depth buffer at work, not draw order.
//
// NOTE a registered onFrame keeps the client presenting every vsync -
// right for a continuously animating scene, wrong for a static one (a
// static scene costs zero passes; drop the onFrame and it idles).
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { box, Group, Mesh, PerspectiveCamera, plane, Scene, sphere, unlit } from "@solidrt/3d"

const SIZE = 720

function App() {
  let [spin, setSpin] = createSignal(0)
  onFrame(tick => setSpin(tick / 2000))

  // Geometries and materials are plain values, shared freely: both boxes
  // use one cube geometry (one vertex/index buffer pair on the GPU), and
  // every mesh here shares the one unlit-color pipeline.
  let cube = box()
  let floor = plane({ width: 6, height: 6, label: "floor" })
  let ball = sphere({ radius: 0.35 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} designSize={[SIZE, SIZE]}>
        <Scene width={SIZE} height={SIZE} clearColor={[0.07, 0.07, 0.1, 1]} label="scene-basic">
          <PerspectiveCamera fov={55} position={[0, 1.6, 3.6]} lookAt={[0, 0.3, 0]} />
          <Mesh
            geometry={floor}
            material={unlit({ color: [0.16, 0.17, 0.22] })}
            rotation={[-Math.PI / 2, 0, 0]}
          />
          <Group rotation={[0, spin(), 0]}>
            <Mesh geometry={cube} material={unlit({ color: [0.85, 0.3, 0.3] })} position={[0, 0.5, 0]} />
            <Mesh
              geometry={cube}
              material={unlit({ color: [0.9, 0.8, 0.35] })}
              position={[-1.1, 0.7, 0]}
              scale={[0.5, 1.4, 0.5]}
            />
            <Mesh geometry={ball} material={unlit({ color: [0.35, 0.65, 0.9] })} position={[1.1, 0.35, 0]} />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
