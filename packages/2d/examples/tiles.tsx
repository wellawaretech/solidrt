// The baked tile layer as a STREAMING world: an unbounded grid of cells
// (negative cells included) baked into chunk textures that the core's
// residency gate brings in as the camera approaches and takes out as it
// leaves, drawn through a view as a few quads. The
// camera flies a ship-style path - a fixed screen pivot near the bottom
// of the viewport, the world panning and ROTATING under it so the flight
// heading always points up (the <TileLayer> camera prop with rotation and
// pivot: a transform write, never a re-bake) - around a ring road many
// times larger than the viewport, so at any moment only the chunks near
// the view are resident; the rest are records on the CPU, taken again
// from the resource pool when the ship comes back round.
//
// The whole world sits 10^7 px from the origin: records are chunk-local,
// so it bakes as crisp as one at the origin, and the view's stage takes
// the camera split by chunk so its floats stay small. A MINIMAP in the
// corner is a second view of the same layer (`<TileLayer layer>`) under
// a north-up, zoomed-out camera, so the world is resident where either
// view wants it, minifying the bake through the chunks' mip chain. The
// layer carries a texture BUDGET (`maxTexels`, 40 chunks' worth): the
// gate evicts the chunk furthest from either view for one more needed
// and the minimap's far chunks wait, stalled, so its edge shows holes
// rather than the main view blurring.
//
// The world is seeded by ONE setTiles rect write over a Uint32Array of
// frame indices (-1 where the world is empty; those chunks never allocate
// records), the shape a generator or a worker produces, with a few cells
// flipped and transposed through the index bits. A timer edits tiles
// while it runs: beacon markers along the road's center line blink, and
// each blink's batch of setTile calls re-bakes only the RESIDENT chunks
// the beacons land in, paced under the bake budget (setBakeBudget: one
// a frame, nearest the ship first, the rest showing their last bake a
// frame longer) - a beacon in a chunk that left is a record write and
// bakes when the chunk comes back.
//
// The atlas is the core logo sliced 2x2 by grid() into 48 px frames, the
// cell size (a frame bakes at its own size); a real game would slice a
// tileset sheet the same way, and the array is the layer's `frames`
// table the indices name.
//
// Read it through the control API while it runs: `/gpu?label=tile-world`
// lists the layer's chunks and `groups["tile-world"]` is its footprint,
// flat while the ship flies; `/stats` has the gate (regionsResident,
// regionsStalled, residentWeight, the window's regionEnters/regionLeaves)
// and the pool (app.poolHits / app.poolMisses: the reuse rate once the
// pool is warm). Debug commands: `state` (the resident chunk count, the
// enters and leaves so far, the pool's figures, the used rect) and `cell`
// ({ col, row }: the frame at a cell, null when empty).
import { createSignal, decodeImage, onFrame, pct, render, windowSize } from "@solidrt/core"
import { poolStats } from "@solidrt/core/gpu"
import { createAtlas, createTileLayer, grid, TileLayer, TILE_FLIP_X, TILE_TRANSPOSE } from "@solidrt/2d"
import type { TileCamera } from "@solidrt/2d"
import { registerDebug } from "sol:dev"
import logoBytes from "./logo.png" with { type: "binary" }

// The logo is 96 px: 2 x 2 frames of 48 px, the cell size.
const TILE = 48
// Chunk edge in tiles: 480 px chunks.
const CHUNK_TILES = 10
// The ring road in cells: centered on the world's center, which sits
// this many cells from the origin (10^7 px at 48 px cells), between
// these radii.
const CENTER_CELL = 208334
const ROAD_INNER = 40
const ROAD_OUTER = 52
// The seeded rect, in cells: one chunk beyond the road each way.
const SEED_REACH = 64
// The texture budget in chunks: the ring's near side under the main view
// plus the minimap's middle, never the whole ring.
const BUDGET_CHUNKS = 40
// What a chunk's mip chain adds to its level-0 texels.
const MIP_CHAIN = 4 / 3
// The minimap: its viewport in layout px and its zoom (world px to view
// px), covering about four chunks across.
const MINIMAP_SIZE = 200
const MINIMAP_ZOOM = 0.1
const MINIMAP_MARGIN = 16

// onFrame tick gap that logs as a hitch, as a factor of the display's
// refresh period (onFrame's rate argument, so any refresh rate works):
// past 1.5 periods a frame slot was missed while this animation ran.
const HITCH_FACTOR = 1.5

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let frames = grid(atlas, 2, 2)

  // The layer is made here, not by a component, so two views can show
  // it: the main view and the minimap both adopt it.
  let chunkPx = CHUNK_TILES * TILE
  let layer = createTileLayer(TILE, TILE, [atlas], {
    frames,
    chunkTiles: CHUNK_TILES,
    maxTexels: Math.ceil(BUDGET_CHUNKS * chunkPx * chunkPx * MIP_CHAIN),
    // Bake at one texel per world pixel on any display: the ship view is
    // at zoom 0.9, so native density, and the budget above is sized for
    // it (a chunk weighs density squared, plus its chain).
    maxDensity: 1,
    label: "tile-world",
  })
  let enters = 0
  let leaves = 0
  layer.onEnter = () => enters++
  layer.onLeave = () => leaves++
  // A solid ring "road" around the center: continuous under the flight
  // path, empty everywhere else - the empty regions are the point, their
  // chunks never hold records, since a rect write makes records only for
  // a chunk the first frame lands in. -1 is the clear (a Uint32Array holds
  // it as 0xffffffff, the same bits). Every eighth road cell is flipped,
  // every ninth transposed, through the index bits.
  let size = 2 * SEED_REACH
  let cells = new Uint32Array(size * size).fill(-1)
  for (let row = -SEED_REACH; row < SEED_REACH; row++) {
    for (let col = -SEED_REACH; col < SEED_REACH; col++) {
      let d = Math.hypot(col, row)
      if (!(d > ROAD_INNER && d < ROAD_OUTER)) continue
      let index = (col ^ row) & 3
      if ((col + row) % 8 === 0) index |= TILE_FLIP_X
      if ((col - row) % 9 === 0) index |= TILE_TRANSPOSE
      cells[(row + SEED_REACH) * size + (col + SEED_REACH)] = index >>> 0
    }
  }
  layer.setTiles(CENTER_CELL - SEED_REACH, CENTER_CELL - SEED_REACH, size, size, cells)
  registerDebug("state", () => ({ resident: layer.resident().length, enters, leaves, pool: poolStats(), used: layer.usedRect() }))
  registerDebug("cell", (args?: { col?: number; row?: number }) => layer.getTile(args?.col ?? CENTER_CELL, args?.row ?? CENTER_CELL))

  // Beacon cells on the road's center line every 7.5 degrees; the interval
  // below blinks them between a marker frame and the road pattern.
  let beacons: [number, number][] = []
  for (let k = 0; k < 48; k++) {
    let a = (k * Math.PI) / 24
    beacons.push([CENTER_CELL + Math.round(Math.cos(a) * 46), CENTER_CELL + Math.round(Math.sin(a) * 46)])
  }

  // The ship flies the ring: camera x/y follow the circle, rotation keeps
  // the heading pointing screen-up, the pivot pins the ship's world point
  // near the viewport bottom. Per-frame camera motion is one signal write
  // feeding the view's camera and the gate - never a re-bake.
  let [camera, setCamera] = createSignal<TileCamera>({})
  let lastTick = 0
  onFrame((tick, _frame, rate) => {
    if (lastTick && tick - lastTick > (1000 / rate) * HITCH_FACTOR) console.log(`[hitch] ${(tick - lastTick).toFixed(1)}ms frame gap`)
    lastTick = tick
    let t = tick / 6000
    let radius = 46 * TILE
    let center = CENTER_CELL * TILE
    // The path r(cos t, sin t) has tangent heading t + pi/2; the camera
    // convention renders a heading h upward at rotation -h - pi/2.
    let heading = t + Math.PI / 2
    setCamera({
      x: center + Math.cos(t) * radius,
      y: center + Math.sin(t) * radius,
      rotation: -heading - Math.PI / 2,
      zoom: 0.9,
      pivotX: windowSize().width / 2,
      pivotY: windowSize().height * 0.78,
    })
  })
  // The minimap's camera: the ship's world point at the minimap's center,
  // north up, zoomed out.
  let minimap = (): TileCamera => ({ x: camera().x, y: camera().y, zoom: MINIMAP_ZOOM, pivotX: MINIMAP_SIZE / 2, pivotY: MINIMAP_SIZE / 2 })

  // Blink the beacons: each 500 ms batch of setTile calls re-bakes only the
  // resident chunks the beacons land in - live edits next to the free
  // scrolling; the rest are record writes.
  let lit = false
  setInterval(() => {
    lit = !lit
    for (let [col, row] of beacons) layer.setTile(col, row, lit ? frames[3]! : frames[(col ^ row) & 3]!)
  }, 500)

  return (
    <window fullscreen>
      <view width={pct(100)} height={pct(100)}>
        {/* Ground color: regions with no cell render nothing, so the
            full-bleed backdrop is the container's, not the layer's. */}
        <d-rect x={0} y={0} w={windowSize().width} h={windowSize().height} color="#0d0d17" />
        <TileLayer layer={layer} camera={camera()} />
        <view position="absolute" x={windowSize().width - MINIMAP_SIZE - MINIMAP_MARGIN} y={MINIMAP_MARGIN} width={MINIMAP_SIZE} height={MINIMAP_SIZE}>
          <d-rect x={0} y={0} w={MINIMAP_SIZE} h={MINIMAP_SIZE} color="#1a1a2a" />
          <TileLayer layer={layer} camera={minimap()} />
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
