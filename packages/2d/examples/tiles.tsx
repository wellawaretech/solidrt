// The baked tile layer: a world of tiles baked into lazily-allocated chunk
// textures and composited as a few quads, so scrolling and rotating cost
// transform writes and an unchanged world costs nothing per frame. The
// world here is BIGGER than one texture is allowed to be (6144px), which is
// the chunking at work; sparse regions never allocate a chunk at all.
//
// The camera flies a ship-style path: a fixed screen pivot near the bottom
// of the viewport, the world panning and ROTATING under it so the flight
// heading always points up - the <TileLayer> camera prop with rotation and
// pivot (a leaf transform, never a re-bake). A timer edits tiles while it
// runs: beacon markers along the road's center line blink, and each blink's
// batch of setTile calls re-bakes only the chunks the beacons land in.
//
// The atlas is the core logo sliced 2x2 by grid(); a real game would slice
// a tileset sheet the same way.
import { createSignal, onFrame, render } from "@solidrt/core"
import { createAtlas, grid, TileLayer } from "@solidrt/2d"
import type { TileCamera, TileLayerHandle } from "@solidrt/2d"
import logoBytes from "./logo.png" with { type: "binary" }

const COLS = 128
const ROWS = 128
const TILE = 48
const VIEW = 720
const WORLD = COLS * TILE

function App() {
  let atlas = createAtlas(logoBytes, { label: "logo-atlas" })
  let frames = grid(2, 2, { width: atlas.width, height: atlas.height })

  let layer!: TileLayerHandle
  let seed = (l: TileLayerHandle) => {
    layer = l
    // A solid ring "road" around the world center: continuous under the
    // flight path, empty everywhere else - the empty regions are the point,
    // their chunks never allocate.
    let c = COLS / 2
    for (let row = 0; row < ROWS; row++) {
      for (let col = 0; col < COLS; col++) {
        let d = Math.hypot(col - c, row - c)
        if (d > 40 && d < 52) l.setTile(col, row, frames[(col ^ row) % 4]!)
      }
    }
  }

  // Beacon cells on the road's center line every 7.5 degrees; the interval
  // below blinks them between a marker frame and the road pattern.
  let beacons: [number, number][] = []
  for (let k = 0; k < 48; k++) {
    let a = (k * Math.PI) / 24
    beacons.push([Math.round(COLS / 2 + Math.cos(a) * 46), Math.round(ROWS / 2 + Math.sin(a) * 46)])
  }

  // The ship flies the ring: camera x/y follow the circle, rotation keeps
  // the heading pointing screen-up, the pivot pins the ship's world point
  // near the viewport bottom. Per-frame camera motion is one signal write
  // feeding the transform - never a re-bake.
  let [camera, setCamera] = createSignal<TileCamera>({})
  onFrame(tick => {
    let t = tick / 6000
    let radius = 46 * TILE
    setCamera({
      x: WORLD / 2 + Math.cos(t) * radius,
      y: WORLD / 2 + Math.sin(t) * radius,
      // Circle tangent heading, rotated so "forward" renders upward.
      rotation: -(t + Math.PI / 2),
      zoom: 0.9,
      pivotX: VIEW / 2,
      pivotY: VIEW * 0.78,
    })
  })

  // Blink the beacons: each 500 ms batch of setTile calls re-bakes only the
  // chunks the beacons land in - live edits next to the free scrolling.
  let lit = false
  setInterval(() => {
    lit = !lit
    for (let [col, row] of beacons) layer.setTile(col, row, lit ? frames[3]! : frames[(col ^ row) % 4]!)
  }, 500)

  return (
    <window alignItems="center" justifyContent="center">
      <view width={VIEW} height={VIEW} overflow="clip">
        {/* Ground color: never-written regions render nothing, so the
            full-bleed backdrop is the container's, not the layer's. */}
        <d-rect x={0} y={0} w={VIEW} h={VIEW} color="#0d0d17" />
        <TileLayer
          cols={COLS}
          rows={ROWS}
          tileW={TILE}
          tileH={TILE}
          atlas={atlas.texture}
          camera={camera()}
          label="tile-world"
          ref={seed}
        />
      </view>
    </window>
  )
}

render(() => <App />)
