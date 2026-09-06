// The 2d camera (createCamera2d) over a world larger than the window: drag
// pans with inertia on release, wheel and pinch zoom about the pointer, a
// tap on empty space glides the view there, and keys switch modes - F
// follows the roaming sprite through a dead zone with damping, R spins
// the view about its center, Space fits the whole world. The camera is
// one setCamera write per changed frame (a shared-params write); no sprite
// moves for the camera's sake. The layer fills the window through the
// function face (starlings' shape): the app owns the <texture> leaf, so
// the camera's handlers spread straight onto it.
//
// Debug commands, for driving it from the control API: `camera` returns
// the pose (and parks it when given x/y/zoom/rotation), `mode` sets
// { follow, spin } or runs { fit: true }.
import { createEffect, displayScale, onFrame, render, windowSize } from "@solidrt/core"
import { addSprite, createAtlas, createCamera2d, createSpriteLayer, fitOversample, grid, setSprite } from "@solidrt/2d"
import type { Camera2dHandle } from "@solidrt/2d"
import { registerDebug } from "srt:dev"
import logoBytes from "./logo.png" with { type: "binary" }

const WORLD = { w: 2400, h: 1600 }
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
const DEAD_ZONE = { w: 0.3, h: 0.3 }
// Largest frame step, seconds: a stall eases on from here instead of
// teleporting.
const MAX_DT = 0.1

let cam!: Camera2dHandle
let following = false
let spinning = false
// The roamer's world position, for the `mode` debug command.
let roamerAt = { x: 0, y: 0 }

function App() {
  let atlas = createAtlas(logoBytes, { label: "logo-atlas" })
  let frames = grid(2, 2, { width: atlas.width, height: atlas.height })
  // The window's logical size, mirrored for the layer and the camera.
  let win = { w: 1, h: 1 }
  let layer = createSpriteLayer(win.w, win.h, atlas.texture, {
    capacity: COUNT + 1,
    clearColor: [0.05, 0.05, 0.09, 1],
    label: "camera",
  })
  for (let i = 0; i < COUNT; i++) {
    addSprite(layer, {
      x: Math.random() * WORLD.w,
      y: Math.random() * WORLD.h,
      w: SPRITE,
      h: SPRITE,
      frame: frames[i % 4],
      rotation: Math.random() * Math.PI * 2,
      tint: [0.6, 0.7, 0.9, 1],
    })
  }
  let roamer = addSprite(layer, { x: WORLD.w / 2, y: WORLD.h / 2, w: ROAMER, h: ROAMER, frame: frames[0], tint: [1, 0.8, 0.3, 1] })

  cam = createCamera2d(layer, {
    viewport: () => win,
    world: WORLD,
    maxZoom: MAX_ZOOM,
    deadZone: DEAD_ZONE,
    onTap: (x, y) => {
      following = false
      cam.glideTo(x, y)
    },
  })

  createEffect(
    () => ({ size: windowSize(), scale: displayScale() }),
    ({ size, scale }) => {
      win.w = Math.max(1, size.width)
      win.h = Math.max(1, size.height)
      layer.setSize(win.w, win.h)
      layer.setOversample(fitOversample(scale, win.w, win.h, win.w * win.h * scale * scale))
    },
  )

  let last = 0
  onFrame(tick => {
    let t = tick / 1000
    let dt = Math.min(MAX_DT, Math.max(0, t - last))
    last = t
    let rx = WORLD.w / 2 + Math.sin(t * ROAM_RATE_X) * WORLD.w * ROAM_SPAN
    let ry = WORLD.h / 2 + Math.sin(t * ROAM_RATE_Y) * WORLD.h * ROAM_SPAN
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
      <texture src={layer.texture} position="absolute" left={0} top={0} width={windowSize().width} height={windowSize().height} {...cam.handlers} />
      <view pointerEvents="none" gap={6} padding={20}>
        <text color="#eef4ff" fontSize={24} fontWeight={700}>
          Camera
        </text>
        <text color="#a9bcd6" fontSize={15}>
          drag to pan (flick for inertia) - wheel or pinch to zoom - tap to glide there - F follow - R spin - Space fit
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
