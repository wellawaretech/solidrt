// A skybox and its reflections: the scene's background sampled from a
// cube map along each pixel's view ray - `<Scene background={{ cube,
// intensity?, rotation? }}>`, Three's `scene.background = cubeTexture` -
// and the same cube as the scene's environment (`environment={{ cube,
// rotation }}`, Three's `scene.environment`), which every lit material
// with `reflectivity` mirrors: the chrome sphere sharply, the brass knot
// blurred by its shininess. The cube map is baked in JS at startup (a
// horizon gradient, a sun disc and its glow), so the example ships no
// image assets; a photographed sky is the same createCubeTexture call
// with six decoded faces in Three's px, nx, py, ny, pz, nz order, or
// equirectToCube on a panorama. The sky turns slowly and the sun light
// turns with it: `rotation` is a reactive prop the scene updates in
// place, no recompile. Drag to look around, wheel to zoom.
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { createCubeTexture } from "@solidrt/core/gpu"
import { box, DirectionalLight, HemisphereLight, lit, Mesh, OrbitCamera, plane, Scene, sphere, torusKnot } from "@solidrt/3d"
import { normalize } from "@solidrt/3d/math"
import type { Vec3 } from "@solidrt/3d/math"

// Face edge in texels; the sun disc is the finest feature, and mipmaps
// keep the horizon smooth when it shrinks toward a screen pixel.
const FACE = 128
// The sun's direction in the unrotated sky.
const SUN: Vec3 = normalize([0.6, 0.35, -0.7])
// Cosine of the disc's angular radius (1.5 degrees).
const SUN_DISC = Math.cos((1.5 * Math.PI) / 180)
// Glow falloff exponent around the disc (higher = tighter).
const SUN_GLOW = 40
// How fast the sky turns about y, radians per second.
const TURN_SPEED = 0.06
// How quickly the ground band darkens below the horizon (per unit of -y).
const GROUND_FALL = 4

type Rgb = [number, number, number]
const ZENITH: Rgb = [0.16, 0.34, 0.74]
const HORIZON: Rgb = [0.8, 0.87, 0.95]
const GROUND: Rgb = [0.3, 0.27, 0.24]
const SUN_COLOR: Rgb = [1, 0.95, 0.82]

let mix = (a: Rgb, b: Rgb, t: number): Rgb => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]

// The sky's color along a world direction.
function sky(d: Vec3): Rgb {
  let y = d[1]
  let rgb = y >= 0 ? mix(HORIZON, ZENITH, Math.sqrt(y)) : mix(HORIZON, GROUND, Math.min(1, -y * GROUND_FALL))
  let s = d[0] * SUN[0] + d[1] * SUN[1] + d[2] * SUN[2]
  if (s > 0) {
    // The disc is the glow at full strength: added to the sky it clamps
    // to white, and the warm tint shows in the halo around it.
    let g = s > SUN_DISC ? 1 : Math.pow(s, SUN_GLOW)
    rgb = [rgb[0] + SUN_COLOR[0] * g, rgb[1] + SUN_COLOR[1] * g, rgb[2] + SUN_COLOR[2] * g]
  }
  return [Math.min(1, rgb[0]), Math.min(1, rgb[1]), Math.min(1, rgb[2])]
}

// The world direction whose lookup lands on face i's texel (s, t), t = 0
// the first row: the GL cube-map table gives the sampling direction, and
// the x flip is the one CUBE_LOOKUP applies (an involution, so applying
// it here bakes the texel the library's lookup will read for `world`).
function texelDirection(face: number, s: number, t: number): Vec3 {
  let a = 2 * s - 1
  let b = 2 * t - 1
  let d: Vec3 =
    face === 0 ? [1, -b, -a] : face === 1 ? [-1, -b, a] : face === 2 ? [a, 1, b] : face === 3 ? [a, -1, -b] : face === 4 ? [a, -b, 1] : [-a, -b, -1]
  d[0] = -d[0]
  return normalize(d)
}

function bakeSky(): Uint8Array[] {
  let faces: Uint8Array[] = []
  for (let face = 0; face < 6; face++) {
    let px = new Uint8Array(FACE * FACE * 4)
    for (let row = 0; row < FACE; row++) {
      for (let col = 0; col < FACE; col++) {
        let rgb = sky(texelDirection(face, (col + 0.5) / FACE, (row + 0.5) / FACE))
        let i = (row * FACE + col) * 4
        px[i] = Math.round(rgb[0] * 255)
        px[i + 1] = Math.round(rgb[1] * 255)
        px[i + 2] = Math.round(rgb[2] * 255)
        px[i + 3] = 255
      }
    }
    faces.push(px)
  }
  return faces
}

// The sun's direction once the sky has turned by `angle` about y (the same
// turn a node with rotation [0, angle, 0] applies).
function turnedSun(angle: number): Vec3 {
  let c = Math.cos(angle)
  let s = Math.sin(angle)
  return [SUN[0] * c + SUN[2] * s, SUN[1], -SUN[0] * s + SUN[2] * c]
}

function App() {
  let cube = createCubeTexture(bakeSky(), FACE, { format: "rgba8-srgb", mipmap: true, label: "sky" })
  let [turn, setTurn] = createSignal(0)
  let last = 0
  onFrame(tick => {
    if (last !== 0) setTurn(turn() + ((tick - last) / 1000) * TURN_SPEED)
    last = tick
  })
  let sun = () => turnedSun(turn())

  let ground = lit({ color: [0.36, 0.4, 0.3] })
  // Glossy: a blurred reflection (shininess 48) at a dielectric's face-on
  // weight, rising toward the silhouette.
  let brass = lit({ color: [0.85, 0.6, 0.3], specular: 0.6, shininess: 48, reflectivity: 0.15 })
  let stone = lit({ color: [0.6, 0.6, 0.62] })
  // Chrome: the environment itself, sharp.
  let chrome = lit({ color: [1, 1, 1], specular: 1, shininess: 400, reflectivity: 1 })
  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene background={{ cube, rotation: turn() }} environment={{ cube, rotation: turn() }} label="skybox-demo">
          <OrbitCamera azimuth={0.4} elevation={0.12} distance={7} />
          <HemisphereLight sky={[0.5, 0.6, 0.8]} ground={[0.3, 0.27, 0.24]} />
          <DirectionalLight direction={[-sun()[0], -sun()[1], -sun()[2]]} color={SUN_COLOR} intensity={0.9} />
          <Mesh geometry={plane({ width: 12, height: 12 })} material={ground} rotation={[-Math.PI / 2, 0, 0]} />
          <Mesh geometry={torusKnot({ radius: 0.7, tube: 0.22, tubularSegments: 128, radialSegments: 16 })} material={brass} position={[0, 1.2, 0]} />
          <Mesh geometry={box()} material={stone} position={[-2, 0.5, -0.5]} />
          <Mesh geometry={sphere({ radius: 0.6, widthSegments: 48, heightSegments: 32 })} material={chrome} position={[2, 0.6, -0.5]} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
