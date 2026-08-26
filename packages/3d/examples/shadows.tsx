// Directional shadow maps: a `castShadow` sun, `castShadow` meshes and
// `lit` materials, which receive shadows by default. The sun swings
// through its arc from onFrame - ONE setTransform on the light node per
// frame; the scene re-places the shadow camera from the light's world
// matrix, so the shadows sweep across the ground while the casters turn
// inside their group. Three's vocabulary throughout: `castShadow` on the
// light and on meshes, `shadow.mapSize/bias/normalBias/camera` on the
// light; the one divergence is that opting OUT of receiving is a material
// option (`lit({ receiveShadow: false })`, Godot's split), because the
// material picks the program (like vertexColors and triplanar).
//
// Placement matters for a casting light: its shadow camera sits at the
// light's WORLD position looking along its direction, so the sun is
// positioned above the scene (a light at the origin would shadow nothing
// above it). The light frustum is `shadow.camera`, +-5 world units by
// default - anything outside it is lit.
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { box, cylinder, DirectionalLight, Group, HemisphereLight, lit, Mesh, PerspectiveCamera, plane, Scene, setTransform, sphere, torusKnot } from "@solidrt/3d"
import type { DirectionalLightNode } from "@solidrt/3d"

const SIZE = 720

function App() {
  let [t, setT] = createSignal(0)
  let sun!: DirectionalLightNode
  onFrame(tick => {
    setT(tick / 1000)
    // The sun's arc: east to west over the scene, always pointing at the
    // origin. The direction is the node's local -y, so aiming is a
    // rotation about z; the position rides the same arc so the shadow
    // camera stays above the casters.
    let a = Math.sin(tick / 4000) * 1.0
    setTransform(sun, { position: [Math.sin(a) * 6, Math.cos(a) * 6, 2], rotation: [0.32, 0, -a] })
  })

  let ground = lit({ color: [0.55, 0.58, 0.6] })
  let red = lit({ color: [0.85, 0.3, 0.25] })
  let blue = lit({ color: [0.3, 0.55, 0.85] })
  let gold = lit({ color: [0.9, 0.75, 0.3], specular: 0.5, shininess: 40 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} viewBox={[SIZE, SIZE]}>
        <Scene width={SIZE} height={SIZE} clearColor={[0.55, 0.65, 0.8, 1]} samples={4} label="shadows">
          <PerspectiveCamera fov={45} position={[0, 4, 7]} lookAt={[0, 0.5, 0]} />
          <HemisphereLight sky={[0.45, 0.5, 0.6]} ground={[0.2, 0.18, 0.15]} />
          <DirectionalLight
            ref={l => (sun = l)}
            color={[1, 0.95, 0.85]}
            intensity={1}
            castShadow
            shadow={{ mapSize: 1024, normalBias: 0.02, camera: { near: 1, far: 20 } }}
          />
          <Mesh geometry={plane({ width: 10, height: 10 })} material={ground} rotation={[-Math.PI / 2, 0, 0]} />
          <Group rotation={[0, t() / 3, 0]}>
            <Mesh geometry={box({ width: 1, height: 1, depth: 1 })} material={red} position={[-1.6, 0.5, 0]} castShadow />
            <Mesh geometry={sphere({ radius: 0.6 })} material={blue} position={[1.5, 0.6, 0]} castShadow />
            <Mesh geometry={torusKnot({ radius: 0.4, tube: 0.14 })} material={gold} position={[0, 1.3, 0]} castShadow />
            <Mesh geometry={cylinder({ radiusTop: 0.12, radiusBottom: 0.12, height: 1.6 })} material={gold} position={[0, 0.8, 0]} castShadow />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
