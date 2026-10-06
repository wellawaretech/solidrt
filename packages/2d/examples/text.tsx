// World-space text: labels that ride the camera as glyph sprites. A sprite
// font (`createSpriteFont`, the runtime's own glyph engine behind
// `flux:font`) is one more atlas in the layer's list, and a text run
// (`addText`, `<Text2d>`) is a group of glyph sprites drawn from it in the
// same draw as the art - so the labels pan, zoom, sort and pick with the
// sprites they belong to, with no per-label JS when the camera moves.
// Each sprite carries a name below it, anchored on its middle; a counter
// at the top re-sets its text every half second (the sprite pool of the
// run is reused, glyphs re-framed in place); the title draws at twice
// the face's size through the component face, a `<Text2d>` under a
// `<SpriteLayer layer={layer} output={false}>` that adopts the same
// imperative layer (the example's own view shows it), the same run under
// props. Wheel or pinch to zoom, drag to pan.
//
// The cells are coverage masks at the face's size for now (the engine's
// distance-field generator is pending, see okf/plans/text-own-rasterizer.md),
// so the labels are exact at 1x and resample under zoom; with "msdf" cells
// one atlas stays sharp across the range, and the `outline` draws.
//
// Debug commands: `zoom` ({ value, x?, y? }) parks the camera and returns
// the pose; `state` returns the counter's text and box, the first label's
// box and sprite count, and the font atlas as it is now.
import { createEffect, createInputMap, createPointerFeed, decodeImage, displayScale, onFrame, render, windowSize } from "@solidrt/core"
import { addSprite, addText, camera2dActions, camera2dBindings, createAtlas, createCamera2d, createSpriteFont, createSpriteLayer, feedPointer, fitOversample, grid, setText, SpriteLayer, Text2d } from "@solidrt/2d"
import type { Camera2dHandle, SpriteFont, TextRun, ViewHandle } from "@solidrt/2d"
import { registerDebug } from "sol:dev"
import logoBytes from "./logo.png" with { type: "binary" }

const WORLD = { width: 1600, height: 1000 }
const COLUMNS = 5
const ROWS = 3
const SPRITE = 72
// The label font: the face's size is the mask cells' size, so labels at
// this size are exact at zoom 1.
const LABEL_PX = 18
// The gap between a sprite's bottom and its label's top.
const LABEL_GAP = 6
// The counter re-sets its text at this interval (milliseconds).
const COUNTER_EVERY_MS = 500
const MIN_ZOOM = 0.2
const MAX_ZOOM = 6
// Largest frame step, seconds: a stall eases on from here.
const MAX_DT = 0.1
const SPRITE_TINT: [number, number, number, number] = [0.6, 0.7, 0.9, 1]
const LABEL_TINT: [number, number, number, number] = [0.93, 0.96, 1, 1]
const COUNTER_TINT: [number, number, number, number] = [1, 0.85, 0.4, 1]
const OUTLINE: { color: [number, number, number]; width: number } = { color: [0.05, 0.05, 0.09], width: 2 }
const NAMES = ["Alder", "Birch", "Cedar", "Elm", "Fir", "Hazel", "Larch", "Maple", "Oak", "Pine", "Rowan", "Spruce", "Willow", "Yew", "Aspen"]

let cam!: Camera2dHandle
let world!: ViewHandle
let font!: SpriteFont
let counter!: TextRun
let labels: TextRun[] = []
let ticks = 0

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let frames = grid(atlas, 2, 2)
  font = createSpriteFont({ fontFamily: "sans", fontSize: LABEL_PX, fontWeight: 600 }, { label: "labels" })
  let win = { width: 1, height: 1 }
  let layer = createSpriteLayer([atlas, font.atlas], { capacity: 512, label: "world", orderBy: "y" })
  world = layer.createView({ width: win.width, height: win.height, clearColor: [0.05, 0.05, 0.09, 1], label: "world" })

  for (let i = 0; i < COLUMNS * ROWS; i++) {
    let x = ((i % COLUMNS) + 0.5) * (WORLD.width / COLUMNS)
    let y = (Math.floor(i / COLUMNS) + 0.5) * (WORLD.height / ROWS)
    addSprite(layer, { x, y, w: SPRITE, h: SPRITE, frame: frames[i % 4], tint: SPRITE_TINT })
    // The name under the sprite, centered on it: the run's anchor is its
    // middle, its top the gap below the sprite.
    labels.push(addText(layer, { font, text: NAMES[i % NAMES.length]!, x, y: y + SPRITE / 2 + LABEL_GAP, anchor: "middle", tint: LABEL_TINT, outline: OUTLINE }))
  }
  counter = addText(layer, { font, text: "0 ticks", x: WORLD.width / 2, y: LABEL_PX, anchor: "middle", tint: COUNTER_TINT, outline: OUTLINE })

  cam = createCamera2d([world], { world: WORLD, minZoom: MIN_ZOOM, maxZoom: MAX_ZOOM })
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
  let nextTick = COUNTER_EVERY_MS
  onFrame(tick => {
    let t = tick / 1000
    let dt = Math.min(MAX_DT, Math.max(0, t - last))
    last = t
    cam.update(dt)
    if (tick >= nextTick) {
      nextTick += COUNTER_EVERY_MS
      ticks++
      setText(counter, { text: `${ticks} ${ticks === 1 ? "tick" : "ticks"}` })
    }
  })

  return (
    <window>
      <texture src={world.texture} position="absolute" left={0} top={0} width={windowSize().width} height={windowSize().height} {...world.handlers} />
      {/* The title, bottom center, bigger than the face: its glyphs scale
          their cells (a mask font resamples; an msdf font stays sharp). The
          component face over the imperative layer: no view of its own, the
          texture above shows it. */}
      <SpriteLayer layer={layer} output={false}>
        <Text2d font={font} text="World-space text" x={WORLD.width / 2} y={WORLD.height - LABEL_PX} fontSize={LABEL_PX * 2} anchor="middle" anchorY="bottom" tint={LABEL_TINT} outline={OUTLINE} />
      </SpriteLayer>
      <view pointerEvents="none" gap={6} padding={20}>
        <text color="#eef4ff" fontSize={24} fontWeight={700}>
          Text
        </text>
        <text color="#a9bcd6" fontSize={15}>
          wheel or pinch to zoom - drag to pan - every label is a group of glyph sprites in the one draw, no JS per label when the camera moves
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

registerDebug("state", () => {
  let first = labels[0]!
  return {
    counter: { text: counter.text, width: counter.width, height: counter.height, sprites: counter.sprites.length },
    label: { text: first.text, width: first.width, height: first.height, sprites: first.sprites.length, visible: first.sprites.filter(s => s._visible).length },
    atlas: { width: font.atlas.width, height: font.atlas.height, size: font.size },
  }
})
