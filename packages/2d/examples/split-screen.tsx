// Split-screen: ONE layer of sprites shown through two views side by
// side, each a full viewport with a camera of its own - the layer has no
// output but its views (a Unity scene renders only through Cameras, a
// Godot World2D only through Viewports). The <SpriteLayer output={false}>
// owns the sprites and no leaf; its two <View2d> children fill the window
// half each and follow one roaming sprite apiece through a <Camera2d>
// with a dead zone, so the two poses differ every frame. Wheel inside a
// pane zooms that pane alone (the follow keeps tracking under the zoom),
// and so does the pad that joined it (press any button on it first);
// a tap on a sprite in either pane tints it in both, since there is only
// one sprite. The `cameras` debug command reads both poses back.
import { createInputMap, createPointerFeed, decodeImage, gamepad, onFrame, render, windowSize, For } from "@solidrt/core"
import { Camera2d, camera2dActions, camera2dBindings, createAtlas, grid, setSprite, Sprite, SpriteLayer, View2d } from "@solidrt/2d"
import type { Camera2dHandle, Frame, SpriteHandle } from "@solidrt/2d"
import { registerDebug } from "srt:dev"
import logoBytes from "./logo.png" with { type: "binary" }

const WORLD = { width: 2400, height: 1600 }
const COUNT = 200
const SPRITE = 48
const ROAMER = 72
// Roaming paths: angular rates (rad/s) and the fraction of the world
// each roamer sweeps.
const ROAM_RATE = [0.23, 0.31, 0.17, 0.29]
const ROAM_SPAN = 0.4
// The follow dead zone, as viewport fractions.
const DEAD_ZONE = { width: 0.2, height: 0.25 }
const PANE_ZOOM = 1.5
const MAX_ZOOM = 4
const TINT: [number, number, number, number] = [0.8, 0.85, 0.95, 1]
const HIT: [number, number, number, number] = [1, 0.5, 0.5, 1]

type Item = { id: number; x: number; y: number; frame: Frame }

let cams: (Camera2dHandle | undefined)[] = [undefined, undefined]
let roamers: (SpriteHandle | undefined)[] = [undefined, undefined]

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let frames = grid(atlas, 2, 2)
  let items: Item[] = []
  for (let i = 0; i < COUNT; i++) {
    items.push({ id: i, x: Math.random() * WORLD.width, y: Math.random() * WORLD.height, frame: frames[i % 4]! })
  }
  // The window halves, live through windowSize (its first read is 0x0,
  // hence the floor of 1): the views resize with the window.
  let pane = () => ({ width: Math.max(1, Math.floor(windowSize().width / 2)), height: Math.max(1, windowSize().height) })
  // The roamers move at frame rate; each <Camera2d> follows its own and
  // runs its frames itself while the follow is settling.
  onFrame(tick => {
    let t = tick / 1000
    for (let i = 0; i < 2; i++) {
      let x = WORLD.width / 2 + Math.sin(t * ROAM_RATE[i * 2]!) * WORLD.width * ROAM_SPAN
      let y = WORLD.height / 2 + Math.cos(t * ROAM_RATE[i * 2 + 1]!) * WORLD.height * ROAM_SPAN
      let roamer = roamers[i]
      if (roamer) setSprite(roamer, { x, y, rotation: t })
      cams[i]?.follow(x, y)
    }
  })
  // Two players, two maps: each pane's <Camera2d> takes its own pane's
  // gestures and its own pad, and nothing crosses over. The pads seat in
  // pick-up order: the first pad to press any button joins pane 0, the
  // next joins pane 1 (gamepad.next(); gamepad(i) would pin the slots).
  let paneView = (i: number) => {
    let pointer = createPointerFeed()
    let input = createInputMap(camera2dActions)
    input.bind(camera2dBindings({ pointer, gamepad: gamepad.next() }))
    return (
      <View2d width={pane().width} height={pane().height} clearColor={i === 0 ? [0.05, 0.05, 0.09, 1] : [0.09, 0.05, 0.05, 1]} label={`pane-${i}`} pointer={pointer}>
        <Camera2d input={input} world={WORLD} zoom={PANE_ZOOM} maxZoom={MAX_ZOOM} deadZone={DEAD_ZONE} ref={c => (cams[i] = c)} />
      </View2d>
    )
  }
  return (
    <window>
      <SpriteLayer atlas={atlas.texture} capacity={COUNT + 2} output={false}>
        <view flexDirection="row" width="100%" height="100%">
          {paneView(0)}
          {paneView(1)}
        </view>
        <For each={items}>
          {item => {
            let handle: SpriteHandle | undefined
            let hit = false
            return (
              <Sprite
                ref={s => (handle = s)}
                x={item.x}
                y={item.y}
                w={SPRITE}
                h={SPRITE}
                frame={item.frame}
                tint={TINT}
                onTap={() => {
                  hit = !hit
                  if (handle) setSprite(handle, { tint: hit ? HIT : TINT })
                }}
              />
            )
          }}
        </For>
        <Sprite ref={s => (roamers[0] = s)} x={WORLD.width / 2} y={WORLD.height / 2} w={ROAMER} h={ROAMER} frame={frames[0]!} tint={[1, 0.8, 0.3, 1]} />
        <Sprite ref={s => (roamers[1] = s)} x={WORLD.width / 2} y={WORLD.height / 2} w={ROAMER} h={ROAMER} frame={frames[3]!} tint={[0.4, 0.9, 1, 1]} />
      </SpriteLayer>
    </window>
  )
}

render(() => <App />)

registerDebug("cameras", () => cams.map(c => c?.camera() ?? null))
