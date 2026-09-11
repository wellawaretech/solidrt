// Tiled views into one atlas, resolved once by the app. Four `<View3d>`
// render `into` an app-owned draw target (one pass for all four, the
// atlas carrying depth and the buffer format), each from its own camera.
// A tile holds LINEAR light like every buffer, so the atlas is displayed
// through a resolve pass of the app's own: `resolveFragment()` (the
// stock resolve source with `uScene` and the RESOLVE set declared) over a
// `createShaderTexture` sampling the atlas, `resolveParams` writing its
// exposure and tone mapping. The scene's own target draws nothing here
// (layer mask 0, no leaf): the tiles are the picture. Godot's SubViewport
// atlas and Unity's camera stacking into one render texture, resolved
// where the runtime displays it.

import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { createDrawTarget, createShaderTexture } from "@solidrt/core/gpu"
import { box, bufferFormat, cone, DirectionalLight, Group, HemisphereLight, lit, Mesh, plane, resolveParams, Scene, sphere, View3d } from "@solidrt/3d"
import { resolveFragment } from "@solidrt/3d/glsl"

// The atlas: two by two tiles.
const TILE_W = 480
const TILE_H = 270
const ATLAS_W = TILE_W * 2
const ATLAS_H = TILE_H * 2
// Each tile's backdrop: a view clears its own rectangle, so the color
// goes on the views (an sRGB color like a scene's clearColor, decoded by
// the library), not on the atlas.
const CLEAR: [number, number, number, number] = [0.12, 0.14, 0.2, 1]
// Where the four cameras stand: a quarter turn apart around the group.
const ORBIT_RADIUS = 6
const ORBIT_HEIGHT = 2.5

function App() {
  let [spin, setSpin] = createSignal(0)
  onFrame(tick => setSpin(tick / 2000))
  let atlas = createDrawTarget(ATLAS_W, ATLAS_H, null, { depth: true, format: bufferFormat(), label: "atlas" })
  // The app's resolve: the whole atlas in one pass, the display pixels.
  let resolved = createShaderTexture(resolveFragment(), ATLAS_W, ATLAS_H, resolveParams({ toneMapping: "aces" }), {
    textures: { uScene: atlas },
    label: "atlas-resolve",
  })
  let floor = plane({ width: 8, height: 8 })
  let tiles = [0, 1, 2, 3].map(i => {
    let angle = (i * Math.PI) / 2
    return {
      x: (i % 2) * TILE_W,
      y: Math.floor(i / 2) * TILE_H,
      camera: { position: [Math.sin(angle) * ORBIT_RADIUS, ORBIT_HEIGHT, Math.cos(angle) * ORBIT_RADIUS] as [number, number, number], target: [0, 0.5, 0] as [number, number, number] },
    }
  })
  return (
    <window>
      <view width={pct(100)} height={pct(100)} designSize={[ATLAS_W, ATLAS_H]}>
        <Scene width={TILE_W} height={TILE_H} layers={0} label="atlas-scene" output={() => <texture src={resolved} width={ATLAS_W} height={ATLAS_H} />}>
          <HemisphereLight sky={[0.6, 0.7, 0.9]} ground={[0.25, 0.2, 0.15]} intensity={0.7} />
          <DirectionalLight direction={[-1, -2, -1]} intensity={1.2} />
          <Mesh geometry={floor} material={lit({ color: [0.55, 0.55, 0.5] })} rotation={[-Math.PI / 2, 0, 0]} />
          <Group rotation={[0, spin(), 0]}>
            <Mesh geometry={box()} material={lit({ color: [0.85, 0.3, 0.3] })} position={[1.2, 0.5, 0]} />
            <Mesh geometry={sphere({ radius: 0.5 })} material={lit({ color: [0.3, 0.6, 0.9], specular: 0.5, shininess: 40 })} position={[-1.2, 0.5, 0]} />
            <Mesh geometry={cone({ radius: 0.5, height: 1.2 })} material={lit({ color: [0.9, 0.8, 0.3] })} position={[0, 0.6, 1.4]} />
          </Group>
          {tiles.map(t => (
            <View3d width={TILE_W} height={TILE_H} into={atlas} x={t.x} y={t.y} clearColor={CLEAR} camera={t.camera} output={() => null} />
          ))}
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
