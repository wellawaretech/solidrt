// Aiming and rotation: a target orbits, and three fixed pointers track it,
// each through a different rotation verb.
//   - lookAt(node, target)      the z-axis rod: one call, world semantics
//   - quatFromTo(y, direction)  the cone: aiming an axis other than +z
//   - quatSlerp damped follow   the lazy cone: visibly lags, then catches up
// All three drive their nodes from onFrame through refs - the frame-rate
// escape hatch - so no per-frame signals exist; the one signal-free scene
// re-renders because setTransform marks the moved nodes dirty.
import { onFrame, pct, render } from "@solidrt/core"
import {
  cone,
  lookAt,
  Mesh,
  PerspectiveCamera,
  plane,
  quat,
  quatFromTo,
  quatSlerp,
  Scene,
  setTransform,
  sphere,
  tube,
  unlit,
  worldPosition,
} from "@solidrt/3d"
import type { MeshNode, Vec3 } from "@solidrt/3d"

const Y_AXIS: Vec3 = [0, 1, 0]

function App() {
  let target!: MeshNode
  let rod!: MeshNode
  let cannon!: MeshNode
  let lazy!: MeshNode

  // Allocated once; every per-frame write reuses them.
  let targetPos: Vec3 = [0, 0, 0]
  let dir: Vec3 = [0, 0, 0]
  let aimQ = quat()

  // A world-space direction from a node to the target: the documented
  // recipe, worldPosition + subtract (exact here - the pointers have no
  // transformed ancestors - and correct even if they get some).
  let aimFrom = (node: MeshNode) => {
    let p = worldPosition(node, dir)
    dir[0] = targetPos[0] - p[0]
    dir[1] = targetPos[1] - p[1]
    dir[2] = targetPos[2] - p[2]
    return dir
  }

  onFrame(tick => {
    let t = tick / 1500
    targetPos[0] = Math.cos(t) * 1.9
    targetPos[1] = 1.1 + Math.sin(t * 0.7) * 0.7
    targetPos[2] = Math.sin(t) * 1.9
    setTransform(target, { position: targetPos })

    // The rod is a +z solid (tube paths run along z): lookAt is the whole
    // aiming story, a world-space point in, done.
    lookAt(rod, targetPos)

    // The cone points along +y, not +z, so lookAt would aim its side.
    // quatFromTo rotates the axis you name onto the direction you want.
    quatFromTo(aimQ, Y_AXIS, aimFrom(cannon))
    setTransform(cannon, { quaternion: aimQ })

    // Damped follow: slerp the CURRENT rotation a fixed fraction of the
    // way toward the aimed one each frame. The 0.04 makes the lag obvious;
    // a real app uses 1 - Math.exp(-k * dt) to stay frame-rate independent.
    quatFromTo(aimQ, Y_AXIS, aimFrom(lazy))
    quatSlerp(aimQ, lazy.quaternion, aimQ, 0.04)
    setTransform(lazy, { quaternion: aimQ })
  })

  let ball = sphere({ radius: 0.22 })
  let pointer = cone({ radius: 0.3, height: 0.9 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.07, 0.07, 0.1, 1]} label="aim">
          <PerspectiveCamera fov={55} position={[0, 2.6, 4.6]} lookAt={[0, 0.7, 0]} />
          <Mesh
            geometry={plane({ width: 7, height: 7, label: "floor" })}
            material={unlit({ color: [0.16, 0.17, 0.22] })}
            rotation={[-Math.PI / 2, 0, 0]}
          />
          <Mesh
            geometry={ball}
            material={unlit({ color: [0.95, 0.85, 0.4] })}
            ref={n => (target = n)}
          />
          <Mesh
            geometry={tube([[0, 0, 0], [0, 0, 1.1]], { radius: 0.09, radialSegments: 10, label: "rod" })}
            material={unlit({ color: [0.85, 0.3, 0.3] })}
            position={[-1.4, 0.5, 0]}
            ref={n => (rod = n)}
          />
          <Mesh
            geometry={pointer}
            material={unlit({ color: [0.35, 0.65, 0.9] })}
            position={[1.4, 0.5, 0]}
            ref={n => (cannon = n)}
          />
          <Mesh
            geometry={pointer}
            material={unlit({ color: [0.45, 0.8, 0.5] })}
            position={[0, 0.5, -1.4]}
            ref={n => (lazy = n)}
          />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
