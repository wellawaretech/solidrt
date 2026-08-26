// Sprites: camera-facing quads turned in the vertex stage, so the camera
// can circle and climb while no per-frame JS touches a single sprite. Two
// billboard modes side by side: "full" glows that stay flat to the screen
// whatever the camera does, and "fixed-y" trees that only yaw to follow
// the camera and stay upright as it climbs - the classic upright sprite.
// The spinning cube is the reference solid; the sprites' `scale` is their
// world size, and their (ignored) rotation is never set.
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { createTexture } from "@solidrt/core/gpu"
import { box, Group, Mesh, PerspectiveCamera, plane, Scene, sprite, Sprite, unlit } from "@solidrt/3d"

const SIZE = 720

// A soft disc, alpha falling off from the center; premultiplied like every
// texture the engine samples.
function glow(): ReturnType<typeof createTexture> {
  let n = 64
  let data = new Uint8Array(n * n * 4)
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      let dx = (x + 0.5) / n - 0.5
      let dy = (y + 0.5) / n - 0.5
      let a = Math.max(0, 1 - Math.sqrt(dx * dx + dy * dy) * 2)
      a = a * a
      data.set([255 * a, 240 * a, 200 * a, 255 * a], (y * n + x) * 4)
    }
  }
  return createTexture(data, n, n)
}

// A cutout tree: green triangle over a brown trunk, transparent elsewhere.
function tree(): ReturnType<typeof createTexture> {
  let n = 64
  let data = new Uint8Array(n * n * 4)
  for (let y = 0; y < n; y++) {
    for (let x = 0; x < n; x++) {
      let u = (x + 0.5) / n
      let v = (y + 0.5) / n
      let inCrown = v < 0.8 && Math.abs(u - 0.5) < v * 0.55
      let inTrunk = v >= 0.8 && Math.abs(u - 0.5) < 0.08
      let rgb = inCrown ? [40, 140, 60] : inTrunk ? [110, 70, 40] : [0, 0, 0]
      let a = inCrown || inTrunk ? 255 : 0
      data.set([rgb[0]!, rgb[1]!, rgb[2]!, a], (y * n + x) * 4)
    }
  }
  return createTexture(data, n, n)
}

function App() {
  let [t, setT] = createSignal(0)
  onFrame(tick => setT(tick / 1000))

  // The camera circles the scene and climbs from eye level to well above
  // it: the glows never change shape, the trees tilt away only as the
  // view looks down on them.
  let eye = () => {
    let a = t() * 0.4
    let elevation = 1.2 + 2.5 * (0.5 - 0.5 * Math.cos(t() * 0.5))
    return [Math.sin(a) * 5, elevation, Math.cos(a) * 5] as [number, number, number]
  }

  let glows = sprite({ map: glow(), color: [1, 0.85, 0.5] })
  let trees = sprite({ map: tree(), billboard: "fixed-y" })
  let ringPositions = Array.from({ length: 8 }, (_, i) => {
    let a = (i / 8) * Math.PI * 2
    return [Math.cos(a) * 1.4, 0.9 + Math.sin(a * 2) * 0.3, Math.sin(a) * 1.4] as [number, number, number]
  })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} viewBox={[SIZE, SIZE]}>
        <Scene width={SIZE} height={SIZE} clearColor={[0.07, 0.07, 0.1, 1]} label="sprites">
          <PerspectiveCamera fov={50} position={eye()} lookAt={[0, 0.6, 0]} />
          <Mesh
            geometry={plane({ width: 8, height: 8 })}
            material={unlit({ color: [0.16, 0.17, 0.22] })}
            rotation={[-Math.PI / 2, 0, 0]}
          />
          <Mesh geometry={box({ width: 0.7, height: 0.7, depth: 0.7 })} material={unlit({ color: [0.85, 0.3, 0.3] })} position={[0, 0.35, 0]} rotation={[0, t(), 0]} />
          <Group rotation={[0, -t() * 0.3, 0]}>
            {ringPositions.map(p => (
              <Sprite material={glows} position={p} scale={0.5} />
            ))}
          </Group>
          <Sprite material={trees} position={[-2.2, 0.8, -1]} scale={[1.2, 1.6, 1]} />
          <Sprite material={trees} position={[2.4, 0.9, 0.5]} scale={[1.4, 1.8, 1]} />
          <Sprite material={trees} position={[0.8, 0.7, -2.6]} scale={[1, 1.4, 1]} />
          <Sprite material={trees} position={[-1.5, 0.6, 2.2]} scale={[0.9, 1.2, 1]} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
