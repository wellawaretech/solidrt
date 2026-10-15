// Contact-hardening soft shadows: a light's SIZE decides its shadows'
// penumbra. The sun on the left has an `angularSize` (degrees, its
// disc's diameter; the real sun's is about 0.53, exaggerated here so the
// effect reads at this scale): the wall's shadow is razor sharp where
// the wall meets the ground and widens toward the shadow of its top
// edge, by the caster-to-receiver distance times the size - the
// percentage-closer soft shadows of Unreal, Godot (light_angular_distance)
// and HDRP. The spot on the right has a `sourceRadius` (world units):
// the post's shadow hardens at its foot and softens toward its tip the
// same way, through the perspective map's estimate. Both run ONE kernel:
// a disc of the scene's `shadows.quality` taps, rotated per pixel, whose
// radius the penumbra sets per fragment; a light without a size filters
// at its `shadow.radius`, and a radius of 1 is the single hardware tap.
// A plank leaning on the wall shows the gradient along one caster.
//
// A click cycles the quality (hard -> low -> medium -> high). The debug
// commands drive it headless: `sun({ angularSize, cascades, intensity })`
// and `spot({ sourceRadius })` set the lights (a nudge of `intensity` is
// how a measurement forces frames out of a still scene), `quality({
// quality })` the tier, `layout` returns where each light's shadow edge meets the ground
// (`foot` beside the caster, `tip` under its top edge: `{ x, z }` on the
// ground, the edge running along x there) and `samples({ x, z0, z1, n })`
// projects a row of ground points along z at `x` to screen pixels, for a
// probe across an edge.
import { createSignal, flush, pct, render } from "@solidrt/core"
import { registerDebug } from "sol:dev"
import { box, DirectionalLight, HemisphereLight, phong, Mesh, PerspectiveCamera, plane, Scene, SpotLight } from "@solidrt/3d"
import type { SceneHandle, ShadowQuality, Vec3 } from "@solidrt/3d"

// The sun's disc, exaggerated from its 0.53 degrees so the penumbra
// shows at this scene's scale; the spot's source radius in world units.
const SUN_ANGULAR_SIZE = 10
const SPOT_SOURCE_RADIUS = 0.3
const SUN_INTENSITY = 0.9
// The sun travels down and toward +x, so the wall's shadow lies along +x.
const SUN_DIRECTION: Vec3 = [1, -1, 0]
const SUN_POSITION: Vec3 = [-10, 10, 0]
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
// Where the probe rows cross each shadow's +z edge: just past the foot of
// the caster, and short of the shadow of its top edge.
const FOOT_IN = 0.2
const TIP_IN = 0.4
// Light layers keep the post out of the sun (bit 1, the sun's) while the
// spot (bits 0 and 1) lights everything, so the post's one shadow on the
// ground is the spot's, with no sun shadow of it crossing the probe rows.
// The scene's own mask holds both bits: a light's shadow view casts the
// meshes the SCENE draws that the light lights.
const SCENE_LAYERS = 3
const SUN_LAYERS = 2
const SPOT_LAYERS = 3
const SUNLIT_LAYERS = 3
const POST_LAYERS = 1
// Map sizes coarse enough that the penumbra cap (SHADOW_MAX_TEXELS map
// texels) leaves the tips visibly wide at this scale.
const SUN_MAP_SIZE = 384
const SPOT_MAP_SIZE = 256

const QUALITIES: ShadowQuality[] = ["hard", "low", "medium", "high"]

let [angularSize, setAngularSize] = createSignal(SUN_ANGULAR_SIZE)
let [cascades, setCascades] = createSignal(1)
let [sunIntensity, setSunIntensity] = createSignal(SUN_INTENSITY)
let [sourceRadius, setSourceRadius] = createSignal(SPOT_SOURCE_RADIUS)
let [quality, setQuality] = createSignal<ShadowQuality>("medium")
let scene: SceneHandle | null = null

let num = (v: unknown, set: (n: number) => void) => {
  if (typeof v === "number") set(v)
}

registerDebug("sun", (args?: Record<string, unknown>) => {
  num(args?.angularSize, setAngularSize)
  num(args?.cascades, setCascades)
  num(args?.intensity, setSunIntensity)
  flush()
  return { angularSize: angularSize(), cascades: cascades(), intensity: sunIntensity() }
})
registerDebug("spot", (args?: Record<string, unknown>) => {
  num(args?.sourceRadius, setSourceRadius)
  flush()
  return { sourceRadius: sourceRadius() }
})
registerDebug("quality", (args?: Record<string, unknown>) => {
  if (typeof args?.quality === "string" && QUALITIES.includes(args.quality as ShadowQuality)) setQuality(args.quality as ShadowQuality)
  flush()
  return { quality: quality() }
})

// The post's shadow edge on the ground at x: the point of the post's +x
// face, at height h, whose ray from the spot lands there, carried on to
// the +z face's landing z (similar triangles from the light's height).
function spotEdge(x: number): { x: number; z: number } {
  let [lx, ly] = SPOT_POSITION
  let scale = (x - lx) / (POST_X + POST_HALF_W - lx)
  let h = ly - ly / scale
  return { x, z: (POST_HALF_D * ly) / (ly - h) }
}

registerDebug("layout", () => {
  let wallFace = WALL_X + WALL_HALF_T
  let postFace = POST_X + POST_HALF_W
  let [lx, ly] = SPOT_POSITION
  // The post's top +x edge lands where its ray from the spot meets the
  // ground (the sun's rays are at 45 degrees, so the wall top lands a
  // wall height out).
  let postTip = lx + ((postFace - lx) * ly) / (ly - POST_H)
  return {
    sun: { foot: { x: wallFace + FOOT_IN, z: WALL_HALF_W }, tip: { x: wallFace + WALL_H - TIP_IN, z: WALL_HALF_W } },
    spot: { foot: spotEdge(postFace + FOOT_IN), tip: spotEdge(postTip - TIP_IN) },
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

  return (
    <window>
      <view width={pct(100)} height={pct(100)} onPointerDown={() => setQuality(q => QUALITIES[(QUALITIES.indexOf(q) + 1) % QUALITIES.length] ?? "medium")}>
        <Scene ref={s => (scene = s)} clearColor={[0.5, 0.6, 0.75, 1]} samples={4} layers={SCENE_LAYERS} shadows={{ quality: quality() }} label="soft-shadows">
          <PerspectiveCamera fov={45} position={[6, 10, 8]} lookAt={[1.5, 0, 0.5]} />
          <HemisphereLight sky={[0.3, 0.32, 0.36]} ground={[0.12, 0.11, 0.1]} />
          <DirectionalLight
            color={[1, 0.96, 0.9]}
            intensity={sunIntensity()}
            position={SUN_POSITION}
            direction={SUN_DIRECTION}
            angularSize={angularSize()}
            layers={SUN_LAYERS}
            castShadow
            shadow={{ mapSize: SUN_MAP_SIZE, normalBias: 0.02, camera: { left: -7, right: 7, top: 7, bottom: -7, near: 1, far: 30 }, cascades: cascades() }}
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
            shadow={{ mapSize: SPOT_MAP_SIZE, normalBias: 0.02, near: 1 }}
          />
          <Mesh geometry={plane({ width: 22, height: 14 })} material={ground} rotation={[-Math.PI / 2, 0, 0]} layers={SUNLIT_LAYERS} />
          <Mesh geometry={box({ width: WALL_HALF_T * 2, height: WALL_H, depth: WALL_HALF_W * 2 })} material={brick} position={[WALL_X, WALL_H / 2, 0]} layers={SUNLIT_LAYERS} castShadow />
          <Mesh geometry={box({ width: 0.2, height: 3, depth: 0.6 })} material={wood} position={[-1.75, 1.35, -1]} rotation={[0, 0, -0.45]} layers={SUNLIT_LAYERS} castShadow />
          <Mesh geometry={box({ width: POST_HALF_W * 2, height: POST_H, depth: POST_HALF_D * 2 })} material={steel} position={[POST_X, POST_H / 2, 0]} layers={POST_LAYERS} castShadow />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
