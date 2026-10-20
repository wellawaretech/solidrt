// A streaming world: a terrain many times larger than what is resident,
// cut into a column grid of cells (createRegionGrid: Unreal's World
// Partition shape) that the spatial core's residency gate brings in as
// the camera approaches and takes out as it leaves. The gate decides and
// paces (every camera is a viewer: the main view AND the minimap want
// cells, and a cell is resident while either does); core's region
// lifecycle loads each cell's data in an ISOLATE (streaming-worker.ts, a
// "use isolate" module: the heightfield, the rock scatter and the mean
// height come back as plain typed arrays, main never waits on the math)
// and then BUILDS the cell under the frame slice budget: the build is a
// generator yielding between the creates (the terrain mesh warmed, then
// the rock population), so a cell's creates spread over frames instead
// of landing in one. While a cell loads and builds its PROXY stands in: a
// flat slab at the cell's mean height (the HLOD rule: drawn until the
// content shows; a stand-in for the cells beyond the budget would be a
// coarser second grid). The grid carries a BYTE
// LIMIT (`limit`, a few cells' worth), so the far cells the minimap wants
// wait, stalled, rather than growing the footprint; the freed buffers go
// through the package's resource pool and come back for the next cell.
//
// The camera flies a loop over the world on its own; a HUD counts the
// cells by state. Read it through the control API while it runs:
// `/stats` has the gate (regionsResident, regionsStalled, residentWeight
// in bytes, the window's regionEnters/regionLeaves), the lifecycle
// (app.regionsLoading, app.regionsBuilding, app.regionsShown), the slice
// queue (app.sliceMs, app.sliceQueued) and the pool (app.poolHits,
// app.poolMisses); `/gpu?label=terrain` lists the cells' resources.
// Debug: `state` (the counts), `park { t }` holds the flight at a loop
// parameter so a snapshot is deterministic, `budget { ms }` sets the
// slice budget.
import { createSignal, onFrame, pct, render, setSliceBudget } from "@solidrt/core"
import { isolate } from "flux:isolate"
import { add, addInstance, createInstancedMesh, createMesh, createRegionGrid, DirectionalLight, disposeInstances, HemisphereLight, icosahedron, plane, Scene, setTransform, standard, View3d } from "@solidrt/3d"
import type { Geometry, RegionGridNode, RegionNode, Vec3 } from "@solidrt/3d"
import { registerDebug } from "sol:dev"
import type * as Worker from "./streaming-worker"

// The world's cells: this many world units across, this many quads per
// side, this noise seed.
const CELL = 64
const RESOLUTION = 16
const SEED = 7
// Load distances: a cell enters within ENTER of a viewer's view volume
// and leaves beyond LEAVE.
const ENTER = 40
const LEAVE = 90
// The grid's byte limit: the estimate per cell (a 16 x 16 heightfield
// with its rocks measures about 23 KB) times this many cells - fewer
// than the main view and the minimap want together, so the far ring
// stalls instead of growing the footprint.
const CELL_BYTES_ESTIMATE = 24 * 1024
const LIMIT_CELLS = 48
// The flight: a loop of this radius around the world's center, at this
// height, one lap in this many seconds, looking this far ahead.
const LOOP_RADIUS = 260
const FLY_HEIGHT = 26
const LAP_SECONDS = 70
const LOOK_AHEAD = 0.08
// The minimap: this many world units across, from this height.
const MAP_SPAN = 420
const MAP_HEIGHT = 300
const MAP_PX = 170
// The main view's far plane.
const FAR = 220
// A rock placement's floats (x, y, z, scale), how deep a rock sits in the
// ground as a fraction of its scale, and its vertical squash.
const ROCK_FLOATS = 4
const ROCK_SINK = 0.3
const ROCK_SQUASH = 0.8
// Frames between HUD recounts.
const RECOUNT_FRAMES = 10
// HUD text size.
const HUD_SIZE = 14

let worker = isolate<typeof Worker>("streaming-worker")

type Cell = Worker.CellData
let grid: RegionGridNode<Cell> | undefined
let parked: number | null = null
let [counts, setCounts] = createSignal({ shown: 0, building: 0, loading: 0, proxies: 0 })

function recount(): void {
  if (grid === undefined) return
  let c = { shown: 0, building: 0, loading: 0, proxies: 0 }
  for (let cell of grid.cells()) {
    if (cell.state === "shown") c.shown++
    else if (cell.state === "building") c.building++
    else if (cell.state === "loading") c.loading++
    if (cell.proxy !== null) c.proxies++
  }
  setCounts(c)
}

registerDebug("state", () => {
  recount()
  return counts()
})
registerDebug("park", (args?: Record<string, unknown>) => {
  parked = typeof args?.t === "number" ? args.t : null
  return { parked }
})
registerDebug("budget", (args?: Record<string, unknown>) => {
  if (typeof args?.ms === "number") setSliceBudget(args.ms)
  return { ok: true }
})

function App() {
  let ground = standard({ color: [0.36, 0.5, 0.26], roughness: 0.95 })
  let rock = standard({ color: [0.45, 0.42, 0.4], roughness: 0.8, instanced: true })
  let slab = standard({ color: [0.25, 0.34, 0.2], roughness: 1 })
  let rockGeometry = icosahedron({ radius: 1, detail: 1 })
  let slabGeometry = plane({ width: CELL, height: CELL })

  let world = createRegionGrid<Cell>({
    cell: [CELL, 0, CELL],
    enter: ENTER,
    leave: LEAVE,
    weight: CELL_BYTES_ESTIMATE,
    limit: CELL_BYTES_ESTIMATE * LIMIT_CELLS,
    label: "terrain",
    // Off the thread: the isolate builds the cell, the data crosses as
    // copies. The signal aborts nothing in there (the call runs to its
    // end), but a result that arrives after the leave is dropped.
    load: region => worker.cell(region.cell![0], region.cell![2], CELL, RESOLUTION, SEED),
    // On the thread, one create-sized step per yield: the terrain mesh
    // (its buffers warmed, so the add uploads nothing), then the rocks.
    build: function* (region: RegionNode<Cell>, data: Cell) {
      let terrain: Geometry = { vertices: data.vertices, indices: data.indices, label: "terrain" }
      region.warm(terrain)
      yield
      add(region.content, createMesh(terrain, ground))
      yield
      let count = data.rocks.length / ROCK_FLOATS
      if (count === 0) return
      // The population's record buffers are mesh-owned: the region
      // disposes them with the cell.
      let rocks = createInstancedMesh(rockGeometry, rock, { capacity: count, label: "terrain" })
      region.own({ dispose: () => disposeInstances(rocks) })
      add(region.content, rocks)
      for (let r = 0; r < count; r++) {
        let at = r * ROCK_FLOATS
        let s = data.rocks[at + ROCK_FLOATS - 1]!
        addInstance(rocks, { position: [data.rocks[at]!, data.rocks[at + 1]! - s * ROCK_SINK, data.rocks[at + 2]!], scale: [s, s * ROCK_SQUASH, s] })
      }
    },
    // The slab: the cell's mean height, drawn while the cell is not shown.
    proxy: region => {
      let m = createMesh(slabGeometry, slab)
      let data = region.data
      setTransform(m, { position: [CELL / 2, data === undefined ? 0 : data.mean, CELL / 2], rotation: [-Math.PI / 2, 0, 0] })
      return m
    },
  })
  grid = world

  // The flight: a loop around the world's center, the camera looking a
  // little ahead along it; the minimap looks straight down from above
  // the camera, north up.
  let [eye, setEye] = createSignal<Vec3>([LOOP_RADIUS, FLY_HEIGHT, 0])
  let [target, setTarget] = createSignal<Vec3>([LOOP_RADIUS, FLY_HEIGHT, -1])
  let t = 0
  let frames = 0
  onFrame((_, __, rate) => {
    if (parked !== null) t = parked
    else t += 1 / (rate * LAP_SECONDS)
    let a = t * Math.PI * 2
    let b = (t + LOOK_AHEAD) * Math.PI * 2
    setEye([Math.cos(a) * LOOP_RADIUS, FLY_HEIGHT, Math.sin(a) * LOOP_RADIUS])
    setTarget([Math.cos(b) * LOOP_RADIUS, FLY_HEIGHT - 6, Math.sin(b) * LOOP_RADIUS])
    if (++frames % RECOUNT_FRAMES === 0) recount()
  })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene
          clearColor={[0.62, 0.74, 0.9, 1]}
          fog={{ color: [0.62, 0.74, 0.9], near: 90, far: FAR }}
          label="streaming"
          camera={{ position: eye(), target: target(), fov: 60, near: 0.5, far: FAR }}
          ref={s => add(s.root, world)}
        >
          <HemisphereLight sky={[0.7, 0.8, 1]} ground={[0.3, 0.28, 0.2]} intensity={0.7} />
          <DirectionalLight direction={[-0.4, -1, -0.3]} color={[1, 0.95, 0.85]} intensity={0.9} />
          <view position="absolute" right={12} top={12} width={MAP_PX} height={MAP_PX}>
            <View3d
              width={MAP_PX}
              height={MAP_PX}
              clearColor={[0.1, 0.12, 0.1, 1]}
              fog={null}
              label="map"
              camera={{
                position: [eye()[0], MAP_HEIGHT, eye()[2]],
                target: [eye()[0], 0, eye()[2]],
                up: [0, 0, -1],
                ortho: { left: -MAP_SPAN / 2, right: MAP_SPAN / 2, top: MAP_SPAN / 2, bottom: -MAP_SPAN / 2 },
                near: 1,
                far: MAP_HEIGHT * 2,
              }}
            />
          </view>
        </Scene>
        <view position="absolute" left={12} top={12} flexDirection="column">
          <text fontSize={HUD_SIZE} color="#ffffff">{`shown ${counts().shown}  building ${counts().building}  loading ${counts().loading}  proxies ${counts().proxies}`}</text>
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
