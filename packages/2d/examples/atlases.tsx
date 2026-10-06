// Several atlases in ONE layer, and an extruded sheet under a zooming
// camera. The layer declares two sheets - the core logo sliced 2x2, and a
// tileset built in code (four solid 32 px cells with a dark one-texel
// rim) - and its sprites draw from either, interleaved by `orderBy: "y"`
// across both sheets in the one draw: the multi-texture batch of PixiJS
// and Phaser, where Unity and Godot would break the batch on every
// texture change. The tileset is uploaded with `mipmap` for the far zoom,
// so it goes through `extrudeGrid` first: every cell gets a 4-texel gutter
// of its own edge pixels, and mip levels 0 through 2 sample clean at the
// cell edges (zoom out and the rims stay crisp; a plain sheet would smear
// each rim with the neighbouring cell's colour). The returned options
// slice the repacked sheet, so nothing downstream knows it was extruded.
//
// Wheel or pinch to zoom, drag empty space to pan.
//
// Debug commands: `zoom` ({ value, x?, y? }) parks the camera zoom (and
// the world point at its pivot) and returns the pose; `atlases` returns
// the layer's atlas list with each sheet's size and the frame each sprite
// kind draws.
import { createEffect, createInputMap, createPointerFeed, decodeImage, displayScale, onFrame, render, windowSize } from "@solidrt/core"
import type { DecodedImage } from "@solidrt/core"
import { addSprite, camera2dActions, camera2dBindings, createAtlas, createCamera2d, createSpriteLayer, extrudeGrid, feedPointer, fitOversample, grid } from "@solidrt/2d"
import type { Camera2dHandle, ViewHandle } from "@solidrt/2d"
import { registerDebug } from "sol:dev"
import logoBytes from "./logo.png" with { type: "binary" }

const WORLD = { width: 1600, height: 1000 }
const COLUMNS = 8
const ROWS = 5
const SPRITE = 96
// The tileset: four solid cells with a one-texel rim, extruded by this
// gutter (a power of two; the cell size is a multiple of it).
const CELL = 32
const CELLS = 4
const GUTTER = 4
const RIM = 1
const CELL_COLORS: [number, number, number][] = [
  [230, 90, 70],
  [90, 200, 120],
  [80, 140, 240],
  [240, 200, 80],
]
const RIM_COLOR: [number, number, number] = [20, 20, 30]
const MIN_ZOOM = 0.05
const MAX_ZOOM = 8
// Largest frame step, seconds: a stall eases on from here.
const MAX_DT = 0.1

let cam!: Camera2dHandle
let world!: ViewHandle

/** The tileset's pixels: `CELLS` solid cells in a row, each rimmed. */
function tileset(): DecodedImage {
  let width = CELLS * CELL
  let data = new Uint8Array(width * CELL * 4)
  for (let cell = 0; cell < CELLS; cell++) {
    let color = CELL_COLORS[cell]!
    for (let y = 0; y < CELL; y++) {
      for (let x = 0; x < CELL; x++) {
        let rim = x < RIM || y < RIM || x >= CELL - RIM || y >= CELL - RIM
        let [r, g, b] = rim ? RIM_COLOR : color
        let i = (y * width + cell * CELL + x) * 4
        data[i] = r
        data[i + 1] = g
        data[i + 2] = b
        data[i + 3] = 255
      }
    }
  }
  return { data, width, height: CELL }
}

function App() {
  let logo = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let logoFrames = grid(logo, 2, 2)
  // The tileset, extruded for its mip chain: the repacked image goes to
  // createAtlas, the returned options to grid.
  let extruded = extrudeGrid(tileset(), CELLS, 1, GUTTER)
  let tiles = createAtlas(extruded.image, { mipmap: true, label: "tiles-atlas" })
  let tileFrames = grid(tiles, CELLS, 1, extruded.options)

  let win = { width: 1, height: 1 }
  // One layer over both sheets; y order interleaves them in the one draw.
  let layer = createSpriteLayer([logo, tiles], { capacity: COLUMNS * ROWS, orderBy: "y", label: "atlases" })
  world = layer.createView({ width: win.width, height: win.height, clearColor: [0.05, 0.05, 0.09, 1], label: "world" })

  for (let i = 0; i < COLUMNS * ROWS; i++) {
    let col = i % COLUMNS
    let row = Math.floor(i / COLUMNS)
    let x = (col + 0.5) * (WORLD.width / COLUMNS)
    // Rows lean so that neighbours overlap and the y order shows.
    let y = (row + 0.5) * (WORLD.height / ROWS) + (col % 2) * SPRITE * 0.4
    let frame = (col + row) % 2 === 0 ? logoFrames[i % 4] : tileFrames[i % CELLS]
    addSprite(layer, { x, y, w: SPRITE, h: SPRITE, frame })
  }

  cam = createCamera2d(world, { world: WORLD, minZoom: MIN_ZOOM, maxZoom: MAX_ZOOM })
  let pointer = createPointerFeed()
  feedPointer(world, pointer)
  let input = createInputMap(camera2dActions)
  input.bind(camera2dBindings({ pointer }))
  input.drive(cam.axes)

  createEffect(
    () => ({ size: windowSize(), scale: displayScale() }),
    ({ size, scale }) => {
      win.width = Math.max(1, size.width)
      win.height = Math.max(1, size.height)
      world.setSize(win.width, win.height)
      world.setOversample(fitOversample(scale, win.width, win.height, win.width * win.height * scale * scale))
    },
  )
  let last = 0
  onFrame(tick => {
    let t = tick / 1000
    let dt = Math.min(MAX_DT, Math.max(0, t - last))
    last = t
    cam.update(dt)
  })
  registerDebug("atlases", () => ({
    atlases: layer.atlases.map(a => ({ texture: a.texture, width: a.width, height: a.height })),
    logoFrame: logoFrames[0],
    tileFrame: tileFrames[0],
    extruded: { width: extruded.image.width, height: extruded.image.height, options: extruded.options },
  }))

  return (
    <window>
      <texture src={world.texture} position="absolute" left={0} top={0} width={windowSize().width} height={windowSize().height} {...world.handlers} />
      <view pointerEvents="none" gap={6} padding={20}>
        <text color="#eef4ff" fontSize={24} fontWeight={700}>
          Atlases
        </text>
        <text color="#a9bcd6" fontSize={15}>
          two sheets, one layer, one draw - wheel or pinch to zoom, drag to pan - the rimmed tiles come from an extruded, mipmapped sheet and stay crisp at any zoom
        </text>
      </view>
    </window>
  )
}

render(() => <App />)

registerDebug("zoom", (args?: { value?: number; x?: number; y?: number }) => {
  let pose: { zoom?: number; x?: number; y?: number } = {}
  if (typeof args?.value === "number") pose.zoom = args.value
  if (typeof args?.x === "number") pose.x = args.x
  if (typeof args?.y === "number") pose.y = args.y
  cam.set(pose)
  return cam.pose()
})
