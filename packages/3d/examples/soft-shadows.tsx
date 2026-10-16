// Contact-hardening soft shadows: a light's SIZE decides its shadows'
// penumbra. The sun on the left has an `angularSize` (degrees, its
// disc's diameter; the real sun's is about 0.53, exaggerated here so the
// effect reads at this scale): the wall's shadow is razor sharp where
// the wall meets the ground and widens toward the shadow of its top
// edge, by the caster-to-receiver distance times the size - the
// percentage-closer soft shadows of Unreal, Godot (light_angular_distance)
// and HDRP. The spot on the right has a `sourceRadius` (world units):
// the post's shadow hardens at its foot and softens toward its tip the
// same way, through the perspective map's estimate. The lamp at the back
// is a point light with a `sourceRadius`, over a pad only it lights, so
// its six face maps show the pair without the sun's shadows crossing it.
// All three run ONE kernel: a disc of the scene's `shadows.quality` taps,
// rotated per pixel, whose radius the penumbra sets per fragment; a
// light without a size filters at its `shadow.radius` (the tier's own
// width when it sets none, 2.5 texels at medium; 1 here, the single
// hardware tap), and the hard tier is that one tap for every light. A
// plank leaning on the wall shows the
// gradient along one caster.
//
// A click cycles the quality (hard -> low -> medium -> high). The debug
// commands drive it headless: `sun({ angularSize, cascades, split,
// intensity, radius })`, `spot({ sourceRadius })` and `lamp({ sourceRadius,
// castShadow })` set the lights (`frames({ count })` draws that many
// consecutive frames of the still scene, nudging the sun's intensity
// each one, so a GPU timing sees a loaded device; `split` is where two
// cascades meet, a fraction of the sun's shadow distance; the lamp's six
// face maps and a second cascade do not fit the scene's 8-slot shadow
// budget together, so a cascade read turns the lamp's shadow off),
// `quality({ quality })` the
// tier, `layout` returns where each light's shadow edge meets the ground
// (`foot` beside the caster, `tip` under its top edge: `{ x, z }` on the
// ground, the edge running along x there) and, with a split set, where
// the sun's first cascade map ends in x (`cascade.edge`), where its blend
// band starts (`inner`) and the band's middle (`band`); `samples({ x,
// z0, z1, n })` projects a row of ground points along z at `x` to screen
// pixels, for a probe across an edge.
import { createSignal, flush, onFrame, pct, render, windowSize } from "@solidrt/core"
import { registerDebug } from "sol:dev"
import { box, DirectionalLight, HemisphereLight, phong, Mesh, PerspectiveCamera, plane, PointLight, Scene, SpotLight } from "@solidrt/3d"
import type { SceneHandle, ShadowQuality, Vec3 } from "@solidrt/3d"
import { cascadeBoundary, frustumSliceSphere, lookAt, mat4, snapToGrid } from "@solidrt/3d/math"

// The sun's disc, exaggerated from its 0.53 degrees so the penumbra
// shows at this scene's scale; the spot's and the lamp's source radius
// in world units (the lamp's is large: its caster is low and close, so
// a wide source is what opens the tip).
const SUN_ANGULAR_SIZE = 10
const SPOT_SOURCE_RADIUS = 0.3
const LAMP_SOURCE_RADIUS = 0.6
const SUN_INTENSITY = 0.9
// The sun travels down and toward +x, so the wall's shadow lies along +x
// and its map's up axis has no z: on the ground that axis is x alone.
const SUN_DIRECTION: Vec3 = [1, -1, 0]
const SUN_POSITION: Vec3 = [-10, 10, 0]
// How far the sun's cascades reach from the camera (its box camera's
// far too), the range a split fraction is taken of.
const SUN_DISTANCE = 30
// The camera, spelled out because the cascade fit is replayed from it.
const CAMERA_POSITION: Vec3 = [6, 10, 8]
const CAMERA_TARGET: Vec3 = [1.5, 0, 0.5]
const CAMERA_FOV = 45
const CAMERA_NEAR = 0.1
const CAMERA_FAR = 100
// The kernel's SHADOW_BLEND: the outer tenth of a cascade map, in map
// units, blends into the next cascade; the band's middle is at half of
// that from the edge.
const CASCADE_BLEND = 0.1
const CASCADE_BAND_MID = CASCADE_BLEND / 2
// The ground: width (x) and depth (z); the back PAD_DEPTH of it is a
// separate plane on the lamp's layer, lit by the lamp and the sky only.
const GROUND_WIDTH = 22
const GROUND_DEPTH = 14
const PAD_DEPTH = 4
const PAD_Z = -GROUND_DEPTH / 2 + PAD_DEPTH / 2
// The wall: centre x, half thickness (x), height, half width (z).
const WALL_X = -2.5
const WALL_HALF_T = 0.15
const WALL_H = 2.5
const WALL_HALF_W = 2.5
// The spot hangs above the post, offset toward -x so the shadow falls to +x.
const SPOT_POSITION: Vec3 = [4, 6, 0]
// The post: centre x, half width (x), height, half depth (z).
const POST_X = 5.5
const POST_HALF_W = 0.2
const POST_H = 3
const POST_HALF_D = 0.6
// The lamp hangs over the pad, offset toward -x from its block the same way.
const LAMP_POSITION: Vec3 = [-1, 6, PAD_Z]
// The block on the pad: centre x, half width (x), height, half depth (z).
const BLOCK_X = 1
const BLOCK_HALF_W = 0.2
const BLOCK_H = 2.5
const BLOCK_HALF_D = 0.6
// Where the probe rows cross each shadow's +z edge: just past the foot of
// the caster, and short of the shadow of its top edge.
const FOOT_IN = 0.2
const TIP_IN = 0.4
// Light layers keep the post out of the sun (bit 1, the sun's) while the
// spot (bits 0 and 1) lights everything on the main ground, so the
// post's one shadow on the ground is the spot's, with no sun shadow of
// it crossing the probe rows; the lamp, its pad and its block are on bit
// 2 alone, so the pad shows the lamp's shadows and nothing else's. The
// scene's own mask holds all three bits: a light's shadow view casts the
// meshes the SCENE draws that the light lights.
const SCENE_LAYERS = 7
const SUN_LAYERS = 2
const SPOT_LAYERS = 3
const SUNLIT_LAYERS = 3
const POST_LAYERS = 1
const LAMP_LAYERS = 4
// Map sizes coarse enough that the penumbra cap (SHADOW_MAX_TEXELS map
// texels) leaves the tips visibly wide at this scale. On texels this
// coarse the tier's own filter width (2.5 texels at medium, a third of
// a metre on the second cascade) would blur every contact, so each light
// pins its floor at the one hardware tap and the foot stays razor sharp;
// `sun({ radius: null })` hands the sun back to the tier.
const SUN_MAP_SIZE = 384
const SPOT_MAP_SIZE = 256
const LAMP_MAP_SIZE = 256
const HARD_FLOOR = 1

const QUALITIES: ShadowQuality[] = ["hard", "low", "medium", "high"]

let [angularSize, setAngularSize] = createSignal(SUN_ANGULAR_SIZE)
let [cascades, setCascades] = createSignal(1)
let [split, setSplit] = createSignal<number | null>(null)
let [sunRadius, setSunRadius] = createSignal<number | null>(HARD_FLOOR)
let [sunIntensity, setSunIntensity] = createSignal(SUN_INTENSITY)
let [sourceRadius, setSourceRadius] = createSignal(SPOT_SOURCE_RADIUS)
let [lampRadius, setLampRadius] = createSignal(LAMP_SOURCE_RADIUS)
let [lampShadow, setLampShadow] = createSignal(true)
let [quality, setQuality] = createSignal<ShadowQuality>("medium")
let scene: SceneHandle | null = null

let num = (v: unknown, set: (n: number) => void) => {
  if (typeof v === "number") set(v)
}

registerDebug("sun", (args?: Record<string, unknown>) => {
  num(args?.angularSize, setAngularSize)
  num(args?.cascades, setCascades)
  num(args?.split, setSplit)
  num(args?.intensity, setSunIntensity)
  if (args?.radius === null) setSunRadius(null)
  num(args?.radius, setSunRadius)
  flush()
  return { angularSize: angularSize(), cascades: cascades(), split: split(), intensity: sunIntensity(), radius: sunRadius() }
})
registerDebug("spot", (args?: Record<string, unknown>) => {
  num(args?.sourceRadius, setSourceRadius)
  flush()
  return { sourceRadius: sourceRadius() }
})
registerDebug("lamp", (args?: Record<string, unknown>) => {
  num(args?.sourceRadius, setLampRadius)
  if (typeof args?.castShadow === "boolean") setLampShadow(args.castShadow)
  flush()
  return { sourceRadius: lampRadius(), castShadow: lampShadow() }
})
// The nudge a forced frame makes: too small to see, enough to re-render.
const FRAME_NUDGE = 0.0001
registerDebug("frames", (args?: Record<string, unknown>) => {
  let count = typeof args?.count === "number" ? args.count : 0
  if (count > 0) {
    let left = count
    let stop = onFrame(() => {
      setSunIntensity(v => (v === SUN_INTENSITY ? SUN_INTENSITY + FRAME_NUDGE : SUN_INTENSITY))
      if (--left <= 0) stop()
    })
  }
  return { count }
})
registerDebug("quality", (args?: Record<string, unknown>) => {
  if (typeof args?.quality === "string" && QUALITIES.includes(args.quality as ShadowQuality)) setQuality(args.quality as ShadowQuality)
  flush()
  return { quality: quality() }
})

// A perspective light's shadow edge on the ground at x, for a caster
// centred at the light's z whose +x face is at `face`: the point of that
// face, at height h, whose ray from the light lands there, carried on to
// the +z face's landing z (similar triangles from the light's height).
function edgeAt(light: Vec3, face: number, halfDepth: number, x: number): { x: number; z: number } {
  let [lx, ly, lz] = light
  let scale = (x - lx) / (face - lx)
  let h = ly - ly / scale
  return { x, z: lz + (halfDepth * ly) / (ly - h) }
}

// The foot and tip rows of a perspective light's caster: the top +x
// edge lands where its ray from the light meets the ground.
function perspectivePair(light: Vec3, face: number, height: number, halfDepth: number): { foot: { x: number; z: number }; tip: { x: number; z: number } } {
  let [lx, ly] = light
  let tip = lx + ((face - lx) * ly) / (ly - height)
  return { foot: edgeAt(light, face, halfDepth, face + FOOT_IN), tip: edgeAt(light, face, halfDepth, tip - TIP_IN) }
}

// The sun's first cascade map, fitted as the scene fits it (the camera
// frustum's slice near..split, its bounding sphere, snapped to the map's
// texel grid in light space), read along the map's up axis and turned
// into x on the ground: where the map ends (`edge`), where its blend
// band starts (`inner`) and the band's middle (`band`). Null until a
// split is set with two or more cascades.
function cascadeBand(): { edge: number; inner: number; band: number } | null {
  let s = split()
  if (cascades() < 2 || s === null) return null
  let size = windowSize()
  let view = lookAt(mat4(), CAMERA_POSITION, CAMERA_TARGET, [0, 1, 0])
  // The lambda only places the practical split; explicit splits ignore it.
  let zf = cascadeBoundary(CAMERA_NEAR, SUN_DISTANCE, 0, cascades(), 0, [s])
  let center: Vec3 = [0, 0, 0]
  let r = frustumSliceSphere(center, { view, eye: CAMERA_POSITION, fov: CAMERA_FOV, ortho: null }, size.width / size.height, CAMERA_NEAR, zf)
  let basis = lookAt(mat4(), [0, 0, 0], SUN_DIRECTION, [0, 1, 0])
  snapToGrid(center, center, basis, (2 * r) / SUN_MAP_SIZE)
  let vc = basis[1] * center[0] + basis[5] * center[1] + basis[9] * center[2]
  let xAt = (v: number) => v / basis[1]
  return { edge: xAt(vc - r), inner: xAt(vc - (1 - CASCADE_BLEND) * r), band: xAt(vc - (1 - CASCADE_BAND_MID) * r) }
}

registerDebug("layout", () => {
  let wallFace = WALL_X + WALL_HALF_T
  return {
    // The sun's rays are at 45 degrees, so the wall top lands a wall height out.
    sun: { foot: { x: wallFace + FOOT_IN, z: WALL_HALF_W }, tip: { x: wallFace + WALL_H - TIP_IN, z: WALL_HALF_W } },
    spot: perspectivePair(SPOT_POSITION, POST_X + POST_HALF_W, POST_H, POST_HALF_D),
    lamp: perspectivePair(LAMP_POSITION, BLOCK_X + BLOCK_HALF_W, BLOCK_H, BLOCK_HALF_D),
    cascade: cascadeBand(),
  }
})
registerDebug("samples", (args?: Record<string, unknown>) => {
  if (scene === null) throw new Error("samples: the scene is not mounted")
  let { x, z0, z1, n } = args ?? {}
  if (typeof x !== "number" || typeof z0 !== "number" || typeof z1 !== "number" || typeof n !== "number" || n < 2) {
    throw new Error("samples: pass { x, z0, z1, n } with n >= 2")
  }
  let points: ({ x: number; y: number } | null)[] = []
  for (let i = 0; i < n; i++) {
    let p = scene.project([x, 0, z0 + ((z1 - z0) * i) / (n - 1)])
    points.push(p === null ? null : { x: p.x, y: p.y })
  }
  return points
})

function App() {
  let ground = phong({ color: [0.62, 0.62, 0.6], specular: 0 })
  let brick = phong({ color: [0.75, 0.45, 0.35], specular: 0 })
  let steel = phong({ color: [0.35, 0.5, 0.75], specular: 0 })
  let wood = phong({ color: [0.8, 0.7, 0.4], specular: 0 })
  let stone = phong({ color: [0.55, 0.55, 0.58], specular: 0 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)} onPointerDown={() => setQuality(q => QUALITIES[(QUALITIES.indexOf(q) + 1) % QUALITIES.length] ?? "medium")}>
        <Scene ref={s => (scene = s)} clearColor={[0.5, 0.6, 0.75, 1]} samples={4} layers={SCENE_LAYERS} shadows={{ quality: quality() }} label="soft-shadows">
          <PerspectiveCamera fov={CAMERA_FOV} near={CAMERA_NEAR} far={CAMERA_FAR} position={CAMERA_POSITION} lookAt={CAMERA_TARGET} />
          <HemisphereLight sky={[0.3, 0.32, 0.36]} ground={[0.12, 0.11, 0.1]} />
          <DirectionalLight
            color={[1, 0.96, 0.9]}
            intensity={sunIntensity()}
            position={SUN_POSITION}
            direction={SUN_DIRECTION}
            angularSize={angularSize()}
            layers={SUN_LAYERS}
            castShadow
            shadow={{
              mapSize: SUN_MAP_SIZE,
              normalBias: 0.02,
              camera: { left: -7, right: 7, top: 7, bottom: -7, near: 1, far: SUN_DISTANCE },
              cascades: cascades(),
              distance: SUN_DISTANCE,
              splits: split() === null ? null : [split()!],
              radius: sunRadius(),
            }}
          />
          <SpotLight
            color={[1, 0.95, 0.85]}
            position={SPOT_POSITION}
            angle={35}
            penumbra={0.05}
            distance={20}
            decay={0}
            sourceRadius={sourceRadius()}
            layers={SPOT_LAYERS}
            castShadow
            shadow={{ mapSize: SPOT_MAP_SIZE, normalBias: 0.02, near: 1, radius: HARD_FLOOR }}
          />
          <PointLight
            color={[0.9, 0.95, 1]}
            intensity={1}
            position={LAMP_POSITION}
            distance={12}
            decay={0}
            sourceRadius={lampRadius()}
            layers={LAMP_LAYERS}
            castShadow={lampShadow()}
            shadow={{ mapSize: LAMP_MAP_SIZE, normalBias: 0.02, near: 1, radius: HARD_FLOOR }}
          />
          <Mesh geometry={plane({ width: GROUND_WIDTH, height: GROUND_DEPTH - PAD_DEPTH })} material={ground} rotation={[-Math.PI / 2, 0, 0]} position={[0, 0, PAD_DEPTH / 2]} layers={SUNLIT_LAYERS} />
          <Mesh geometry={plane({ width: GROUND_WIDTH, height: PAD_DEPTH })} material={stone} rotation={[-Math.PI / 2, 0, 0]} position={[0, 0, PAD_Z]} layers={LAMP_LAYERS} />
          <Mesh geometry={box({ width: WALL_HALF_T * 2, height: WALL_H, depth: WALL_HALF_W * 2 })} material={brick} position={[WALL_X, WALL_H / 2, 0]} layers={SUNLIT_LAYERS} castShadow />
          <Mesh geometry={box({ width: 0.2, height: 3, depth: 0.6 })} material={wood} position={[-1.75, 1.35, -1]} rotation={[0, 0, -0.45]} layers={SUNLIT_LAYERS} castShadow />
          <Mesh geometry={box({ width: POST_HALF_W * 2, height: POST_H, depth: POST_HALF_D * 2 })} material={steel} position={[POST_X, POST_H / 2, 0]} layers={POST_LAYERS} castShadow />
          <Mesh geometry={box({ width: BLOCK_HALF_W * 2, height: BLOCK_H, depth: BLOCK_HALF_D * 2 })} material={brick} position={[BLOCK_X, BLOCK_H / 2, PAD_Z]} layers={LAMP_LAYERS} castShadow />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
