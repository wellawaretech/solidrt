// Layer views (layer.createView): the world rendered twice from one set of
// sprites - the window-filling main view under a pannable, zoomable camera
// and a corner MINIMAP showing the whole world, no sprite mirrored. A view
// is one more target over the layer's own instance buffers: the sprites
// are added once, the roamer moves once, and both renderings follow at
// zero per-frame JS beyond each camera's writes. The minimap carries the
// layer's pointer dispatch under its own camera: a tap on it glides the
// main camera to that world point (the view's root listener, `e.x`/`e.y`
// in world pixels), and a tap on a sprite in the minimap selects it as it
// would in the main view (the same sprite handlers, the walk ending at
// the VIEW). A stroked rect over the minimap outlines the main camera's
// view rect through `map.project`.
//
// Debug commands: `state` returns both cameras and the outline rect;
// `camera` parks the main camera when given x/y/zoom and returns its
// pose; `selected` the selected sprite's world position (null when none),
// `first` the first sprite's - the one to aim a synthetic tap at.
import { createEffect, createInputMap, createPointerFeed, createSignal, decodeImage, displayScale, gamepad, onFrame, render, windowSize } from "@solidrt/core"
import { addSprite, camera2dActions, camera2dBindings, createAtlas, createCamera2d, createSpriteLayer, feedPointer, fitOversample, grid, setSprite } from "@solidrt/2d"
import type { Camera2dHandle, SpriteHandle, ViewHandle } from "@solidrt/2d"
import { registerDebug } from "srt:dev"
import logoBytes from "./logo.png" with { type: "binary" }

const WORLD = { width: 2400, height: 1600 }
const COUNT = 300
const SPRITE = 48
const ROAMER = 72
const MAX_ZOOM = 4
// The minimap's pixel size: the whole world fitted into it.
const MAP_WIDTH = 240
const MAP_HEIGHT = 160
const MAP_MARGIN = 16
const TINT: [number, number, number, number] = [0.75, 0.8, 0.9, 1]
const SELECTED_TINT: [number, number, number, number] = [1, 0.4, 0.4, 1]
// Roamer figure-eight: rates in rad/s and the span as a fraction of the world.
const ROAM_RATE_X = 0.23
const ROAM_RATE_Y = 0.31
const ROAM_SPAN = 0.4
// Cap on a frame's dt in seconds, so a resumed app cannot leap.
const MAX_DT = 0.1

let cam!: Camera2dHandle
let map!: ViewHandle
let selected: SpriteHandle | null = null
let first: SpriteHandle | null = null
let outline = { x: 0, y: 0, width: 0, height: 0 }

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let frames = grid(atlas, 2, 2)
  let win = { width: 1, height: 1 }
  let layer = createSpriteLayer(atlas.texture, { capacity: COUNT + 1, label: "world" })
  // The main view: the window, under the pannable camera.
  let main = layer.createView({ width: win.width, height: win.height, clearColor: [0.05, 0.05, 0.09, 1], label: "world" })
  let select = (sprite: SpriteHandle | null) => {
    if (selected) setSprite(selected, { tint: TINT })
    selected = sprite
    if (selected) setSprite(selected, { tint: SELECTED_TINT })
  }
  for (let i = 0; i < COUNT; i++) {
    let sprite = addSprite(layer, {
      x: Math.random() * WORLD.width,
      y: Math.random() * WORLD.height,
      w: SPRITE,
      h: SPRITE,
      frame: frames[i % 4],
      rotation: Math.random() * Math.PI * 2,
      tint: TINT,
    })
    sprite.onTap = () => select(sprite)
    if (first === null) first = sprite
  }
  let roamer = addSprite(layer, { x: WORLD.width / 2, y: WORLD.height / 2, w: ROAMER, h: ROAMER, frame: frames[0], tint: [1, 0.8, 0.3, 1] })

  // The minimap: the whole world at the fit zoom, world (0, 0) at the
  // view's top-left (the default pivot), its own clear color.
  let mapZoom = Math.min(MAP_WIDTH / WORLD.width, MAP_HEIGHT / WORLD.height)
  map = layer.createView({
    width: MAP_WIDTH,
    height: MAP_HEIGHT,
    camera: { zoom: mapZoom },
    clearColor: [0.02, 0.02, 0.05, 0.85],
    label: "minimap",
  })

  cam = createCamera2d(main, { viewport: () => win, world: WORLD, maxZoom: MAX_ZOOM })
  // The main view's gestures and any pad drive the camera through a map;
  // the minimap binds nothing (its tap glides through listen below).
  let pointer = createPointerFeed()
  feedPointer(main, pointer)
  let input = createInputMap(camera2dActions)
  input.bind(camera2dBindings({ pointer, gamepad: gamepad() }))
  input.drive(cam.axes)
  main.listen({
    onTap: e => {
      if (e.sprite === null) select(null)
    },
  })
  // A tap on the minimap, sprite or not, glides the main camera there:
  // the view's dispatch undoes the MAP camera, so e.x/e.y are world pixels.
  map.listen({ onTap: e => cam.glideTo(e.x, e.y) })

  createEffect(
    () => ({ size: windowSize(), scale: displayScale() }),
    ({ size, scale }) => {
      win.width = Math.max(1, size.width)
      win.height = Math.max(1, size.height)
      main.setSize(win.width, win.height)
      let budget = win.width * win.height * scale * scale
      main.setOversample(fitOversample(scale, win.width, win.height, budget))
      map.setOversample(fitOversample(scale, MAP_WIDTH, MAP_HEIGHT, budget))
    },
  )

  // The main view's rect on the minimap, as a signal written only when the
  // camera moved this frame.
  let [rect, setRect] = createSignal(outline)
  let last = 0
  onFrame(tick => {
    let t = tick / 1000
    let dt = Math.min(MAX_DT, Math.max(0, t - last))
    last = t
    let rx = WORLD.width / 2 + Math.sin(t * ROAM_RATE_X) * WORLD.width * ROAM_SPAN
    let ry = WORLD.height / 2 + Math.sin(t * ROAM_RATE_Y) * WORLD.height * ROAM_SPAN
    setSprite(roamer, { x: rx, y: ry, rotation: t })
    cam.update(dt)
    let view = cam.viewRect()
    let [x0, y0] = map.project(view.x, view.y)
    let [x1, y1] = map.project(view.x + view.width, view.y + view.height)
    if (x0 !== outline.x || y0 !== outline.y || x1 - x0 !== outline.width || y1 - y0 !== outline.height) {
      outline = { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }
      setRect(outline)
    }
  })

  return (
    <window>
      <texture src={main.texture} position="absolute" left={0} top={0} width={windowSize().width} height={windowSize().height} {...main.handlers} />
      <view pointerEvents="none" gap={6} padding={20}>
        <text color="#eef4ff" fontSize={24} fontWeight={700}>
          Views
        </text>
        <text color="#a9bcd6" fontSize={15}>
          drag to pan, wheel or pinch to zoom - tap the minimap to glide there - tap a sprite in either view to select it
        </text>
      </view>
      <view position="absolute" right={MAP_MARGIN} bottom={MAP_MARGIN} width={MAP_WIDTH} height={MAP_HEIGHT} overflow="clip">
        <texture src={map.texture} width={MAP_WIDTH} height={MAP_HEIGHT} {...map.handlers} />
        <rect
          position="absolute"
          left={rect().x}
          top={rect().y}
          width={rect().width}
          height={rect().height}
          drawStyle="stroke"
          strokeWidth={1}
          color="#ffffff"
          pointerEvents="none"
        />
      </view>
    </window>
  )
}

render(() => <App />)

registerDebug("state", () => ({ camera: cam.camera(), map: map.camera(), outline }))
registerDebug("camera", (args?: Record<string, unknown>) => {
  let pose: Record<string, number> = {}
  for (let key of ["x", "y", "zoom"]) {
    if (typeof args?.[key] === "number") pose[key] = args[key] as number
  }
  if (Object.keys(pose).length > 0) cam.set(pose)
  return cam.camera()
})
registerDebug("selected", () => (selected && selected.layer !== null ? { x: selected._x, y: selected._y } : null))
registerDebug("first", () => (first ? { x: first._x, y: first._y } : null))
