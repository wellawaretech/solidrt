// The baked tile layer as a STREAMING world: an unbounded grid of cells
// (negative cells included) baked into chunk textures that the core's
// residency gate brings in as the camera approaches and takes out as it
// leaves, composited as a few quads under a clipping viewport. The
// camera flies a ship-style path - a fixed screen pivot near the bottom
// of the viewport, the world panning and ROTATING under it so the flight
// heading always points up (the <TileLayer> camera prop with rotation and
// pivot: a transform write, never a re-bake) - around a ring road many
// times larger than the viewport, so at any moment only the chunks near
// the view are resident; the rest are records on the CPU, taken again
// from the resource pool when the ship comes back round.
//
// The world is seeded by ONE setTiles rect write over a Uint16Array of
// frame indices (-1 where the world is empty; those chunks never allocate
// records), the shape a generator or a worker produces, with a few cells
// flipped and transposed through the index bits. A timer edits tiles
// while it runs: beacon markers along the road's center line blink, and
// each blink's batch of setTile calls re-bakes only the RESIDENT chunks
// the beacons land in - a beacon in a chunk that left is a record write
// and bakes when the chunk comes back.
//
// The atlas is the core logo sliced 2x2 by grid() into 48 px frames, the
// cell size (a frame bakes at its own size); a real game would slice a
// tileset sheet the same way, and the array is the layer's `frames`
// table the indices name.
//
// Debug commands: `state` (the resident chunk count, the enters and
// leaves so far, the used rect) and `cell` ({ col, row }: the frame at a
// cell, null when empty).
import { createSignal, decodeImage, onFrame, pct, render, windowSize } from "@solidrt/core"
import { createAtlas, grid, TileLayer, TILE_FLIP_X, TILE_TRANSPOSE } from "@solidrt/2d"
import type { TileCamera, TileLayerHandle } from "@solidrt/2d"
import { registerDebug } from "sol:dev"
import logoBytes from "./logo.png" with { type: "binary" }

// The logo is 96 px: 2 x 2 frames of 48 px, the cell size.
const TILE = 48
// The ring road in cells: centered on the origin, so the world spans
// negative cells too, between these radii.
const ROAD_INNER = 40
const ROAD_OUTER = 52
// The seeded rect, in cells: one chunk beyond the road each way.
const SEED_REACH = 64

// onFrame tick gap that logs as a hitch, as a factor of the display's
// refresh period (onFrame's rate argument, so any refresh rate works):
// past 1.5 periods a frame slot was missed while this animation ran.
const HITCH_FACTOR = 1.5

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let frames = grid(atlas, 2, 2)

  let layer!: TileLayerHandle
  let enters = 0
  let leaves = 0
  let seed = (l: TileLayerHandle) => {
    layer = l
    // A solid ring "road" around the origin: continuous under the flight
    // path, empty everywhere else - the empty regions are the point,
    // their chunks never hold records, since a rect write makes records
    // only for a chunk the first frame lands in. -1 is the clear (a
    // Uint16Array holds it as 0xffff, the same bits). Every eighth road
    // cell is flipped, every ninth transposed, through the index bits.
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
    l.setTiles(-SEED_REACH, -SEED_REACH, size, size, cells)
  }
  registerDebug("state", () => ({ resident: layer.resident().length, enters, leaves, used: layer.usedRect() }))
  registerDebug("cell", (args?: { col?: number; row?: number }) => layer.getTile(args?.col ?? 0, args?.row ?? 0))

  // Beacon cells on the road's center line every 7.5 degrees; the interval
  // below blinks them between a marker frame and the road pattern.
  let beacons: [number, number][] = []
  for (let k = 0; k < 48; k++) {
    let a = (k * Math.PI) / 24
    beacons.push([Math.round(Math.cos(a) * 46), Math.round(Math.sin(a) * 46)])
  }

  // The ship flies the ring: camera x/y follow the circle, rotation keeps
  // the heading pointing screen-up, the pivot pins the ship's world point
  // near the viewport bottom. Per-frame camera motion is one signal write
  // feeding the transform and the viewer - never a re-bake.
  let [camera, setCamera] = createSignal<TileCamera>({})
  let lastTick = 0
  onFrame((tick, _frame, rate) => {
    if (lastTick && tick - lastTick > (1000 / rate) * HITCH_FACTOR) console.log(`[hitch] ${(tick - lastTick).toFixed(1)}ms frame gap`)
    lastTick = tick
    let t = tick / 6000
    let radius = 46 * TILE
    // The path r(cos t, sin t) has tangent heading t + pi/2; the camera
    // convention renders a heading h upward at rotation -h - pi/2.
    let heading = t + Math.PI / 2
    setCamera({
      x: Math.cos(t) * radius,
      y: Math.sin(t) * radius,
      rotation: -heading - Math.PI / 2,
      zoom: 0.9,
      pivotX: windowSize().width / 2,
      pivotY: windowSize().height * 0.78,
    })
  })

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
        <TileLayer
          tileW={TILE}
          tileH={TILE}
          atlases={[atlas]}
          frames={frames}
          camera={camera()}
          onEnter={() => enters++}
          onLeave={() => leaves++}
          label="tile-world"
          ref={seed}
        />
      </view>
    </window>
  )
}

render(() => <App />)
