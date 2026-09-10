// The rover drawn solid, as a wireframe or as its feature edges. Both line
// forms are GEOMETRY: wireframeGeometry lists every edge of a triangle
// geometry once, edgesGeometry only the edges where two faces meet at the
// threshold angle or more (plus the open borders), each as a "lines"
// geometry over the source's OWN vertex array. The topology rides on the
// geometry, so the plain unlit material draws it as lines, and the vertex
// upload is shared, so a mode costs an index buffer, not a second model.
// The swap is setGeometry + setMaterial on the live mesh: the node, its
// transform and its pointer handlers are untouched. Space, a tap on the
// rover or the `wire` debug command ({ mode }) cycles the modes.
//
// Under the rover, the debug helpers: gridHelper on its floor and
// axesHelper at the origin (world space, so they stand still while the
// rover turns) and box3Helper around its bounds, turning with it. Lines
// geometry too; the two colored ones draw through one
// unlit({ vertexColors: true }) material.

import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { createTexture } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { registerDebug } from "srt:dev"
import { add, axesHelper, box3Helper, createModel, DirectionalLight, edgesGeometry, equirectToCube, gridHelper, Group, Mesh, parseGltf, PerspectiveCamera, Scene, setGeometry, setMaterial, unlit, wireframeGeometry } from "@solidrt/3d"
import type { SceneNode } from "@solidrt/3d"
import modelBytes from "./model.glb" with { type: "binary" }

const MODES = ["solid", "wireframe", "edges"] as const
type Mode = (typeof MODES)[number]
// Feature-edge threshold in degrees: above the facet angle of the rover's
// rounded parts, so their seams stay hidden and only rims and creases draw.
const EDGE_ANGLE = 20
const LINE_COLOR: [number, number, number] = [0.85, 0.95, 1]
// The grid's subdivision lines and its two center lines, and the bounds
// box (Three's Box3Helper yellow), against the dark clear color.
const GRID_COLOR: [number, number, number] = [0.3, 0.33, 0.4]
const GRID_CENTER_COLOR: [number, number, number] = [0.6, 0.66, 0.8]
const BOUNDS_COLOR: [number, number, number] = [1, 1, 0]
// Grid cells across the rover's floor.
const GRID_DIVISIONS = 20
// Face edge of the gradient environment cube: a gradient needs no detail.
const ENV_FACE = 16
// The gradient, zenith to nadir, sRGB bytes: sky, horizon, ground, below.
const ENV_ROWS: [number, number, number][] = [
  [96, 122, 168],
  [176, 190, 210],
  [120, 105, 90],
  [60, 52, 45],
]

// A tiny equirectangular panorama turned into the cube the solid rover's
// standard materials light by (see model.tsx).
function gradientEnvironment(): TextureId {
  let px = new Uint8Array(2 * ENV_ROWS.length * 4)
  ENV_ROWS.forEach((c, y) => {
    for (let x = 0; x < 2; x++) px.set([c[0], c[1], c[2], 255], (y * 2 + x) * 4)
  })
  let panorama = createTexture(px, 2, ENV_ROWS.length, { format: "rgba8-srgb", filter: "linear" })
  return equirectToCube(panorama, ENV_FACE, { format: "rgba8-srgb", mipmap: true, label: "gradient-env" })
}

function App() {
  let cube = gradientEnvironment()
  let [t, setT] = createSignal(0)
  onFrame(tick => setT(tick / 1000))

  let model = createModel(parseGltf(modelBytes), { label: "rover" })
  let b = model.bounds
  let center: [number, number, number] = [(b[0]! + b[3]!) / 2, (b[1]! + b[4]!) / 2, (b[2]! + b[5]!) / 2]
  let radius = Math.hypot(b[3]! - b[0]!, b[4]! - b[1]!, b[5]! - b[2]!) / 2

  let helperLines = unlit({ vertexColors: true })
  let grid = gridHelper({ size: radius * 4, divisions: GRID_DIVISIONS, color: GRID_COLOR, centerColor: GRID_CENTER_COLOR })
  let axes = axesHelper({ size: radius })
  let bounds = box3Helper(b)

  // One line material for every part and both line modes. Per part, what
  // each mode swaps in: the line geometries share the solid's vertices,
  // so building both up front costs their index buffers only.
  let lines = unlit({ color: LINE_COLOR })
  let looks = model.parts.map(part => ({
    mesh: part.mesh,
    solid: { geometry: part.mesh.geometry, material: part.mesh.material },
    wireframe: wireframeGeometry(part.mesh.geometry),
    edges: edgesGeometry(part.mesh.geometry, EDGE_ANGLE),
  }))
  let mode: Mode = "solid"
  let apply = (next: Mode): void => {
    mode = next
    for (let look of looks) {
      // Each call rebuilds the draw entry, so the intermediate state is
      // always the line material over the solid's triangles - a pipeline
      // unlit has anyway - never the solid material over lines.
      if (next === "solid") {
        setGeometry(look.mesh, look.solid.geometry)
        setMaterial(look.mesh, look.solid.material)
      } else {
        setMaterial(look.mesh, lines)
        setGeometry(look.mesh, look[next])
      }
    }
  }
  let cycle = (): void => apply(MODES[(MODES.indexOf(mode) + 1) % MODES.length]!)
  for (let part of model.parts) part.mesh.onTap = cycle
  registerDebug("wire", (args?: Record<string, unknown>) => {
    if (typeof args?.mode === "string" && (MODES as readonly string[]).includes(args.mode)) apply(args.mode as Mode)
    else cycle()
  })

  return (
    <window
      onKeyDown={e => {
        if (e.key === " ") cycle()
      }}
    >
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.1, 0.11, 0.14, 1]} environment={{ cube }} samples={4} label="wireframe">
          <PerspectiveCamera fov={40} position={[center[0] + radius * 1.4, center[1] + radius * 1.1, center[2] + radius * 2.2]} lookAt={center} />
          <DirectionalLight direction={[0.5, -0.8, 0.3]} color={[1, 0.95, 0.85]} intensity={0.9} />
          <Mesh geometry={grid} material={helperLines} position={[0, b[1]!, 0]} />
          <Mesh geometry={axes} material={helperLines} />
          <Group rotation={[0, t() / 3, 0]} ref={(g: SceneNode) => add(g, model)}>
            <Mesh geometry={bounds} material={unlit({ color: BOUNDS_COLOR })} />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
