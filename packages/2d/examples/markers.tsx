// Screen-size markers and an additive glow layer over one world. Two
// sprite layers share ONE camera (createCamera2d over both views keeps
// them in step; the glow view's leaf takes no pointer): the world layer
// holds the sprites, each with a PIN at its corner drawn at a constant 18
// px (minScreenPx = maxScreenPx, the gizmo: `w`/`h` carry the aspect
// alone), a LABEL bar over every fourth sprite that never grows past 24
// px however far in you zoom (maxScreenPx alone), and the selection RING
// that never shrinks under 48 px however far out (minScreenPx alone), so
// the selected sprite stays findable at the overview. The glow layer is
// the same atlas on a `blend: "add"` pipeline: half-lit haloes that
// accumulate to white where they overlap, over the transparent clear.
// Wheel or pinch to zoom, drag empty space to pan, tap a sprite to
// select it (the ring glides along), tap empty space to deselect.
// Picking follows the drawn size: `pick` takes the view's zoom, so a tap
// lands on the ring where the ring is drawn.
//
// Debug commands: `zoom` ({ value, x?, y? }) parks the camera zoom (and
// the world point at its pivot) and returns the pose; `select` ({ index })
// selects a sprite (null clears) and
// returns its world position, where the world view shows it (`at`, view
// pixels) and the ring's; `pick` ({ x, y }) returns
// what the world view hits at those VIEW pixels, as marker kinds.
import { createEffect, createInputMap, createPointerFeed, decodeImage, displayScale, onFrame, render, windowSize } from "@solidrt/core"
import { addSprite, camera2dActions, camera2dBindings, createAtlas, createCamera2d, createSpriteLayer, feedPointer, fitOversample, grid, setSprite, setSpriteTransition } from "@solidrt/2d"
import type { Camera2dHandle, SpriteHandle, ViewHandle } from "@solidrt/2d"
import { registerDebug } from "sol:dev"
import logoBytes from "./logo.png" with { type: "binary" }

const WORLD = { width: 1600, height: 1000 }
const COLUMNS = 6
const ROWS = 4
const SPRITE = 64
// The pin: a constant 18 px gizmo at the sprite's top-right corner.
const PIN_PX = 18
// The label bar over every fourth sprite: world-sized until the zoom
// would grow it past 24 px tall.
const LABEL_EVERY = 4
const LABEL = { w: 120, h: 24 }
const LABEL_MAX_PX = 24
// The selection ring: world-sized until the zoom would shrink it under
// 48 px.
const RING = 96
const RING_MIN_PX = 48
// A halo on the glow layer, wider than the sprite spacing so neighbours
// overlap, a quarter lit: where four meet they add up to white.
const HALO = 320
const HALO_TINT: [number, number, number, number] = [0.3, 0.2, 0.1, 0.25]
const MIN_ZOOM = 0.08
const MAX_ZOOM = 8
// The ring's glide to a newly selected sprite.
const RING_GLIDE = "260ms ease-out"
// Largest frame step, seconds: a stall eases on from here.
const MAX_DT = 0.1
const SPRITE_TINT: [number, number, number, number] = [0.6, 0.7, 0.9, 1]
const PIN_TINT: [number, number, number, number] = [1, 0.45, 0.35, 1]
const LABEL_TINT: [number, number, number, number] = [0.35, 0.9, 0.6, 0.9]
const RING_TINT: [number, number, number, number] = [1, 0.9, 0.4, 0.6]

type Kind = "sprite" | "pin" | "label" | "ring"

let cam!: Camera2dHandle
let world!: ViewHandle
let sprites: SpriteHandle[] = []
let kinds = new Map<SpriteHandle, { kind: Kind; index: number }>()
let ring!: SpriteHandle
let selected: number | null = null

function select(index: number | null) {
  selected = index
  if (index === null) {
    setSprite(ring, { visible: false })
    return
  }
  let s = sprites[index]!
  setSprite(ring, { x: s._x, y: s._y, visible: true })
}

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let frames = grid(atlas, 2, 2)
  let win = { width: 1, height: 1 }
  let layer = createSpriteLayer([atlas], { capacity: 128, label: "markers" })
  let glow = createSpriteLayer([atlas], { capacity: 64, blend: "add", label: "glow" })
  world = layer.createView({ width: win.width, height: win.height, clearColor: [0.05, 0.05, 0.09, 1], label: "world" })
  let glowView = glow.createView({ width: win.width, height: win.height, clearColor: [0, 0, 0, 0], label: "glow" })

  for (let i = 0; i < COLUMNS * ROWS; i++) {
    let x = ((i % COLUMNS) + 0.5) * (WORLD.width / COLUMNS)
    let y = (Math.floor(i / COLUMNS) + 0.5) * (WORLD.height / ROWS)
    let sprite = addSprite(layer, { x, y, w: SPRITE, h: SPRITE, frame: frames[i % 4], tint: SPRITE_TINT })
    sprite.onTap = () => select(i)
    sprites.push(sprite)
    kinds.set(sprite, { kind: "sprite", index: i })
    // The constant-size pin at the corner: equal bounds, the aspect from
    // w/h (a square), drawn over the sprite.
    let pin = addSprite(layer, { x: x + SPRITE / 2, y: y - SPRITE / 2, w: 1, h: 1, frame: frames[1], tint: PIN_TINT, minScreenPx: PIN_PX, maxScreenPx: PIN_PX })
    kinds.set(pin, { kind: "pin", index: i })
    if (i % LABEL_EVERY === 0) {
      let label = addSprite(layer, { x, y: y - SPRITE, w: LABEL.w, h: LABEL.h, frame: frames[2], tint: LABEL_TINT, maxScreenPx: LABEL_MAX_PX })
      kinds.set(label, { kind: "label", index: i })
    }
    addSprite(glow, { x, y, w: HALO, h: HALO, frame: frames[3], tint: HALO_TINT })
  }
  // The ring: added last so it draws over everything, hidden until a tap.
  ring = addSprite(layer, { x: 0, y: 0, w: RING, h: RING, frame: frames[0], tint: RING_TINT, minScreenPx: RING_MIN_PX, visible: false })
  setSpriteTransition(ring, { position: RING_GLIDE })
  kinds.set(ring, { kind: "ring", index: -1 })

  // One camera, both views: the glow follows the world exactly.
  cam = createCamera2d([world, glowView], { viewport: () => win, world: WORLD, minZoom: MIN_ZOOM, maxZoom: MAX_ZOOM })
  let pointer = createPointerFeed()
  feedPointer(world, pointer)
  let input = createInputMap(camera2dActions)
  input.bind(camera2dBindings({ pointer }))
  input.drive(cam.axes)
  world.listen({
    onTap: e => {
      if (e.sprite === null) select(null)
    },
  })

  createEffect(
    () => ({ size: windowSize(), scale: displayScale() }),
    ({ size, scale }) => {
      win.width = Math.max(1, size.width)
      win.height = Math.max(1, size.height)
      let oversample = fitOversample(scale, win.width, win.height, win.width * win.height * scale * scale)
      for (let view of [world, glowView]) {
        view.setSize(win.width, win.height)
        view.setOversample(oversample)
      }
    },
  )
  let last = 0
  onFrame(tick => {
    let t = tick / 1000
    let dt = Math.min(MAX_DT, Math.max(0, t - last))
    last = t
    cam.update(dt)
  })

  return (
    <window>
      <texture src={world.texture} position="absolute" left={0} top={0} width={windowSize().width} height={windowSize().height} {...world.handlers} />
      {/* The glow leaf composites over the world (alpha, its transparent
          clear) and takes no pointer, so every tap reaches the world. */}
      <texture src={glowView.texture} position="absolute" left={0} top={0} width={windowSize().width} height={windowSize().height} pointerEvents="none" />
      <view pointerEvents="none" gap={6} padding={20}>
        <text color="#eef4ff" fontSize={24} fontWeight={700}>
          Markers
        </text>
        <text color="#a9bcd6" fontSize={15}>
          wheel or pinch to zoom - drag to pan - tap a sprite to ring it - pins hold 18 px, labels never pass 24 px, the ring never drops under 48 px - haloes add up
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

registerDebug("select", (args?: { index?: number | null }) => {
  if (args && "index" in args) select(args.index ?? null)
  let s = selected === null ? null : sprites[selected]!
  return {
    selected,
    sprite: s ? { x: s._x, y: s._y, at: world.project(s._x, s._y) } : null,
    ring: { x: ring._x, y: ring._y, visible: ring._visible },
  }
})

registerDebug("pick", (args?: { x?: number; y?: number }) => {
  let x = args?.x ?? 0
  let y = args?.y ?? 0
  return { zoom: world.camera().zoom, hits: world.pick(x, y).map(s => kinds.get(s) ?? null) }
})
