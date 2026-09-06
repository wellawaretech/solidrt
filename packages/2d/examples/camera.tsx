// The 2d camera (createCamera2d) over a world larger than the window,
// attached at the layer's root: drag empty space to pan with inertia on
// release, wheel or pinch anywhere to zoom about the pointer, tap empty
// space to glide the view there. The sprites take part in the same
// walk: a tap on one selects it (tint), a drag on one moves it - the
// sprite stops its down, which claims the press, so the camera never
// pans under a sprite drag. Keys switch modes - F follows the roaming
// sprite through a dead zone with damping, R spins the view about its
// center, Space fits the whole world. The camera is one setCamera write
// per changed frame (a shared-params write); no sprite moves for the
// camera's sake. The layer fills the window through the function face
// (starlings' shape): the app owns the <texture> leaf and spreads the
// LAYER's handlers on it; the camera listens at the root through attach.
//
// Debug commands, for driving it from the control API: `camera` returns
// the pose (and parks it when given x/y/zoom/rotation), `mode` sets
// { follow, spin } or runs { fit: true }, `selected` returns the selected
// sprite's world position (null when none), `first` the first sprite's -
// the one to aim synthetic taps and drags at.
import { createEffect, displayScale, onFrame, render, windowSize } from "@solidrt/core"
import { addSprite, createAtlas, createCamera2d, createSpriteLayer, fitOversample, grid, setSprite } from "@solidrt/2d"
import type { Camera2dHandle, SpriteHandle } from "@solidrt/2d"
import { registerDebug } from "srt:dev"
import logoBytes from "./logo.png" with { type: "binary" }

const WORLD = { width: 2400, height: 1600 }
const COUNT = 300
const SPRITE = 64
const ROAMER = 96
const MAX_ZOOM = 4
// The roamer's Lissajous path: radians per second per axis, and how much
// of the world it sweeps.
const ROAM_RATE_X = 0.23
const ROAM_RATE_Y = 0.31
const ROAM_SPAN = 0.4
// Spin rate while R is on, radians per second.
const SPIN_RATE = 0.4
// The follow dead zone, fractions of the viewport.
const DEAD_ZONE = { width: 0.3, height: 0.3 }
// Largest frame step, seconds: a stall eases on from here instead of
// teleporting.
const MAX_DT = 0.1
const TINT: [number, number, number, number] = [0.6, 0.7, 0.9, 1]
const SELECTED_TINT: [number, number, number, number] = [1, 0.95, 0.5, 1]

let cam!: Camera2dHandle
let following = false
let spinning = false
// The roamer's world position, for the `mode` debug command.
let roamerAt = { x: 0, y: 0 }
let selected: SpriteHandle | null = null
let first: SpriteHandle | null = null
// The sprite holding a press (onPointerMove also fires on plain hover;
// only the captured moves drag).
let dragging: SpriteHandle | null = null

function App() {
  let atlas = createAtlas(logoBytes, { label: "logo-atlas" })
  let frames = grid(2, 2, { width: atlas.width, height: atlas.height })
  // The window's logical size, mirrored for the layer and the camera.
  let win = { width: 1, height: 1 }
  let layer = createSpriteLayer(win.width, win.height, atlas.texture, {
    capacity: COUNT + 1,
    clearColor: [0.05, 0.05, 0.09, 1],
    label: "camera",
  })
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
    // The sprite claims its press: the camera at the root never sees this
    // pointer again, so dragging the sprite moves it instead of the view.
    sprite.onPointerDown = e => {
      e.stopPropagation()
      dragging = sprite
    }
    sprite.onPointerUp = () => (dragging = null)
    sprite.onPointerMove = e => {
      if (dragging === sprite) setSprite(sprite, { x: e.x, y: e.y })
    }
    sprite.onTap = () => select(sprite)
    if (first === null) first = sprite
  }
  let roamer = addSprite(layer, { x: WORLD.width / 2, y: WORLD.height / 2, w: ROAMER, h: ROAMER, frame: frames[0], tint: [1, 0.8, 0.3, 1] })

  cam = createCamera2d(layer, {
    viewport: () => win,
    world: WORLD,
    maxZoom: MAX_ZOOM,
    deadZone: DEAD_ZONE,
  })
  cam.attach(layer)
  // A tap that reached the root landed on empty space (a sprite's tap
  // still bubbles here with e.sprite set, so the miss is the null case).
  layer.listen({
    onTap: e => {
      if (e.sprite) return
      select(null)
      following = false
      cam.glideTo(e.x, e.y)
    },
  })

  createEffect(
    () => ({ size: windowSize(), scale: displayScale() }),
    ({ size, scale }) => {
      win.width = Math.max(1, size.width)
      win.height = Math.max(1, size.height)
      layer.setSize(win.width, win.height)
      layer.setOversample(fitOversample(scale, win.width, win.height, win.width * win.height * scale * scale))
    },
  )

  let last = 0
  onFrame(tick => {
    let t = tick / 1000
    let dt = Math.min(MAX_DT, Math.max(0, t - last))
    last = t
    let rx = WORLD.width / 2 + Math.sin(t * ROAM_RATE_X) * WORLD.width * ROAM_SPAN
    let ry = WORLD.height / 2 + Math.sin(t * ROAM_RATE_Y) * WORLD.height * ROAM_SPAN
    setSprite(roamer, { x: rx, y: ry, rotation: t })
    roamerAt.x = rx
    roamerAt.y = ry
    if (following) cam.follow(rx, ry)
    if (spinning) cam.set({ rotation: cam.camera().rotation! + SPIN_RATE * dt })
    cam.update(dt)
  })

  return (
    <window
      onKeyDown={e => {
        if (e.key === "f") {
          following = !following
          if (!following) cam.unfollow()
        } else if (e.key === "r") {
          spinning = !spinning
        } else if (e.key === " ") {
          following = false
          cam.unfollow()
          cam.fit(undefined, { glide: true })
        }
      }}
    >
      <texture src={layer.texture} position="absolute" left={0} top={0} width={windowSize().width} height={windowSize().height} {...layer.handlers} />
      <view pointerEvents="none" gap={6} padding={20}>
        <text color="#eef4ff" fontSize={24} fontWeight={700}>
          Camera
        </text>
        <text color="#a9bcd6" fontSize={15}>
          drag empty space to pan (flick for inertia) - wheel or pinch to zoom - tap empty space to glide there - tap a sprite to select, drag it to move - F follow - R spin - Space fit
        </text>
      </view>
    </window>
  )
}

render(() => <App />)

registerDebug("camera", (args?: Record<string, unknown>) => {
  let pose: Record<string, number> = {}
  for (let key of ["x", "y", "zoom", "rotation"]) {
    if (typeof args?.[key] === "number") pose[key] = args[key] as number
  }
  if (Object.keys(pose).length > 0) cam.set(pose)
  return cam.camera()
})

registerDebug("mode", (args?: Record<string, unknown>) => {
  if (typeof args?.follow === "boolean") {
    following = args.follow
    if (!following) cam.unfollow()
  }
  if (typeof args?.spin === "boolean") spinning = args.spin
  if (args?.fit === true) cam.fit(undefined, { glide: true })
  return { following, spinning, roamer: { ...roamerAt } }
})

registerDebug("selected", () => (selected ? { x: selected._x, y: selected._y } : null))
registerDebug("first", () => (first ? { x: first._x, y: first._y } : null))
