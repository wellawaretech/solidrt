// The standard material: the sphere grid every PBR engine's docs open
// with - metalness down the rows, roughness across the columns - under a
// baked sky as background and environment (the skybox example's bake) and
// a warm sun casting shadows. What to look for: the bare-metal row IS the
// environment, sharp at the left and blurred toward the right, tinted by
// the base color and dark wherever the sky is; the dielectric row keeps
// its diffuse color and shows the sky as a faint gloss, a mirror highlight
// at the left that widens to the right and brightens toward every
// silhouette; the rows between blend the two. standard takes lit's base,
// maps, cutout, shadow, emissive and fog options - only the Blinn-Phong
// knobs are gone, replaced by metalness/roughness and their packed maps.
// Light intensities read as lit's: 1 lights a white matte surface to 1.
// Drag to look around.

import { pct, render } from "@solidrt/core"
import { createCubeTexture } from "@solidrt/core/gpu"
import { DirectionalLight, HemisphereLight, Mesh, OrbitCamera, plane, Scene, sphere, standard } from "@solidrt/3d"
import { normalize } from "@solidrt/3d/math"
import type { Vec3 } from "@solidrt/3d/math"

// Metalness steps down the rows, roughness across the columns.
const ROWS = 4
const COLS = 6
// Sphere radius and grid pitch, world units.
const RADIUS = 0.42
const PITCH = 1.05
// The spheres' base color, sRGB: a warm red that reads as paint and as metal.
const COLOR: [number, number, number] = [0.8, 0.25, 0.2]
// Height of the bottom row's centers above the ground.
const LIFT = 0.6
// Face edge of the baked sky in texels.
const FACE = 128
// The sun's direction (toward it).
const SUN: Vec3 = normalize([0.6, 0.45, -0.7])
// How far out the sun light sits, so its shadow camera covers the grid.
const SUN_DISTANCE = 12
// Cosine of the sun disc's angular radius (1.5 degrees).
const SUN_DISC = Math.cos((1.5 * Math.PI) / 180)
// Glow falloff exponent around the disc (higher = tighter).
const SUN_GLOW = 40
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
    let g = s > SUN_DISC ? 1 : Math.pow(s, SUN_GLOW)
    rgb = [rgb[0] + SUN_COLOR[0] * g, rgb[1] + SUN_COLOR[1] * g, rgb[2] + SUN_COLOR[2] * g]
  }
  return [Math.min(1, rgb[0]), Math.min(1, rgb[1]), Math.min(1, rgb[2])]
}

// The world direction whose lookup lands on face i's texel (s, t) - the
// GL cube-map table, no flip (see skybox.tsx).
function texelDirection(face: number, s: number, t: number): Vec3 {
  let a = 2 * s - 1
  let b = 2 * t - 1
  let d: Vec3 =
    face === 0 ? [1, -b, -a] : face === 1 ? [-1, -b, a] : face === 2 ? [a, 1, b] : face === 3 ? [a, -1, -b] : face === 4 ? [a, -b, 1] : [-a, -b, -1]
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

function App() {
  let cube = createCubeTexture(bakeSky(), FACE, { format: "rgba8-srgb", mipmap: true, label: "sky" })
  let ball = sphere({ radius: RADIUS, widthSegments: 48, heightSegments: 32 })
  let ground = standard({ color: [0.32, 0.33, 0.3], roughness: 0.9 })
  // One material per cell: metalness by row (top row bare metal, bottom
  // row dielectric), roughness by column (mirror at the left).
  let cells: { material: ReturnType<typeof standard>; position: Vec3 }[] = []
  for (let r = 0; r < ROWS; r++) {
    for (let c = 0; c < COLS; c++) {
      cells.push({
        material: standard({ color: COLOR, metalness: 1 - r / (ROWS - 1), roughness: c / (COLS - 1) }),
        position: [(c - (COLS - 1) / 2) * PITCH, LIFT + (ROWS - 1 - r) * PITCH, 0],
      })
    }
  }
  let center: Vec3 = [0, LIFT + ((ROWS - 1) * PITCH) / 2, 0]
  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene background={{ cube }} environment={{ cube }} samples={4} label="standard-demo">
          <OrbitCamera target={center} azimuth={0.25} elevation={0.15} distance={9} />
          <HemisphereLight sky={[0.5, 0.6, 0.8]} ground={[0.3, 0.27, 0.24]} intensity={0.5} />
          <DirectionalLight
            position={[SUN[0] * SUN_DISTANCE, SUN[1] * SUN_DISTANCE, SUN[2] * SUN_DISTANCE]}
            direction={[-SUN[0], -SUN[1], -SUN[2]]}
            color={SUN_COLOR}
            intensity={1}
            castShadow
          />
          <Mesh geometry={plane({ width: 16, height: 16 })} material={ground} rotation={[-Math.PI / 2, 0, 0]} />
          {cells.map(cell => (
            <Mesh geometry={ball} material={cell.material} position={cell.position} castShadow />
          ))}
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
