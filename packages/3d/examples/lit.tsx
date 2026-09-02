// The lit material and the scene's lights. lit() is the standard look -
// hemisphere ambient, the scene's directional lights, Lambert diffuse,
// optional Blinn-Phong highlight - with the same color/map/transparent
// options as unlit. Lights are NODES: <HemisphereLight> plus up to four
// <DirectionalLight>s, here a warm key inside a spinning <Group> (a
// parent's rotation turns the light, no per-frame direction math) and a
// fixed cool fill; however many meshes, a light change is one shared
// uniform write. The three boxes share one checker map: the
// UV-mapped one stretches it per face (every generator emits 0..1 UVs),
// the two triplanar ones tile it at one world density whatever their
// size - the reason triplanar is an option on lit.

import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { createTexture } from "@solidrt/core/gpu"
import { box, DirectionalLight, Group, HemisphereLight, lit, Mesh, PerspectiveCamera, Scene, sphere, torusKnot, plane } from "@solidrt/3d"

function checker(): ReturnType<typeof createTexture> {
  let n = 64
  let data = new Uint8Array(n * n * 4)
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      let v = ((x >> 3) + (y >> 3)) & 1 ? 235 : 120
      data.set([v, v, v, 255], (y * n + x) * 4)
    }
  }
  return createTexture(data, n, n, { wrap: "repeat", mipmap: true })
}

function App() {
  let [t, setT] = createSignal(0)
  onFrame(tick => setT(tick / 1000))

  let map = checker()
  let uvMapped = lit({ map })
  let tiled = lit({ map, triplanar: 2 })
  let glossy = lit({ color: [0.85, 0.3, 0.25], specular: 0.6, shininess: 60 })
  let matte = lit({ color: [0.3, 0.55, 0.85] })
  let glass = lit({ color: [0.9, 0.95, 1, 0.35], specular: 1, shininess: 120, transparent: true })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.07, 0.07, 0.1, 1]} samples={4} label="lit">
          <PerspectiveCamera fov={50} position={[0, 2.6, 5]} lookAt={[0, 0.4, 0]} />
          <HemisphereLight sky={[0.35, 0.38, 0.45]} ground={[0.12, 0.1, 0.08]} />
          <Group rotation={[0, t(), 0]}>
            <DirectionalLight direction={[0.7, -0.8, 0]} color={[1, 0.92, 0.8]} intensity={0.9} />
          </Group>
          <DirectionalLight direction={[0.6, -0.3, 0.5]} color={[0.4, 0.5, 0.8]} intensity={0.35} />
          <Mesh geometry={plane({ width: 8, height: 8 })} material={tiled} rotation={[-Math.PI / 2, 0, 0]} />
          <Group rotation={[0, t() / 4, 0]}>
            <Mesh geometry={box({ width: 0.8, height: 0.8, depth: 0.8 })} material={uvMapped} position={[-1.6, 0.4, 0]} />
            <Mesh geometry={box({ width: 1.6, height: 0.5, depth: 0.8 })} material={tiled} position={[0, 0.25, 1.2]} />
            <Mesh geometry={sphere({ radius: 0.5 })} material={glossy} position={[1.5, 0.5, 0]} />
            <Mesh geometry={torusKnot({ radius: 0.35, tube: 0.12 })} material={matte} position={[0, 0.9, -1]} />
            <Mesh geometry={sphere({ radius: 0.45 })} material={glass} position={[0, 0.45, 0]} />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
