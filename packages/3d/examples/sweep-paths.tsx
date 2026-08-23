// Swept solids along 3D polylines. sweep() runs a 2D profile along a
// path with mitred joints: bare path points crease - the strap folds
// over the crate's edges like real webbing - while smooth-tagged points
// share averaged normals, so the helix sweeps into ONE continuous coil
// (per-segment boxes with unmitred gaps are exactly what this replaces).
// tube() is the round-profile shorthand. lit() materials on purpose:
// creased vs smooth joints differ only in normals, which unlit color
// would hide.
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { box, DirectionalLight, Group, HemisphereLight, lit, Mesh, PerspectiveCamera, plane, roundRect, Scene, sweep, tube, unlit } from "@solidrt/3d"
import type { SweepPath } from "@solidrt/3d"

const SIZE = 720

function App() {
  let [spin, setSpin] = createSignal(0)
  onFrame(tick => setSpin(tick / 4000))

  // The strap hugs the crate at half its thickness; every bend is a bare
  // (sharp) point, so each fold creases exactly on a crate edge.
  let o = 0.013
  let strapPath: SweepPath = [
    [-1.75, o, 0],
    [-1.3 - o, o, 0],
    [-1.3 - o, 0.6 + o, 0],
    [-0.3 + o, 0.6 + o, 0],
    [-0.3 + o, o, 0],
    [0.15, o, 0],
  ]
  let strap = sweep(roundRect(0.3, 0.026, 0.008), strapPath, { label: "strap" })

  // A smooth-tagged helix: one continuous tube, not a stack of segments.
  let coilPath: SweepPath = []
  for (let i = 0; i <= 60; i++) {
    let a = (i / 60) * Math.PI * 5
    coilPath.push({ p: [0.85 + Math.cos(a) * 0.35, 0.055 + i * 0.0095, Math.sin(a) * 0.35], smooth: true })
  }
  let coil = tube(coilPath, { radius: 0.05, radialSegments: 12, label: "coil" })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} viewBox={[SIZE, SIZE]}>
        <Scene width={SIZE} height={SIZE} clearColor={[0.07, 0.07, 0.1, 1]} label="sweep-paths">
          <PerspectiveCamera fov={55} position={[0, 1.9, 3.9]} lookAt={[0, 0.35, 0]} />
          <HemisphereLight sky={[0.45, 0.45, 0.45]} ground={[0.22, 0.22, 0.22]} />
          <DirectionalLight direction={[-0.5, -0.8, -0.4]} intensity={0.8} />
          <Mesh
            geometry={plane({ width: 6, height: 6, label: "floor" })}
            material={unlit({ color: [0.16, 0.17, 0.22] })}
            rotation={[-Math.PI / 2, 0, 0]}
          />
          <Group rotation={[0, spin(), 0]}>
            <Mesh geometry={box({ width: 1, height: 0.6, depth: 0.8 })} material={lit({ color: [0.55, 0.42, 0.28] })} position={[-0.8, 0.3, 0]} />
            <Mesh geometry={strap} material={lit({ color: [0.9, 0.55, 0.2] })} />
            <Mesh geometry={coil} material={lit({ color: [0.45, 0.6, 0.8] })} />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
