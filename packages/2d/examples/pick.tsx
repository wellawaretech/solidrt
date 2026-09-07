// The layer's event model through the component face: the built-in leaf
// carries the layer's handlers, so <Sprite onPointer*> props receive events
// with LAYER-pixel coordinates and exact rotated-rect hit testing, topmost
// sprite first, and the walk ends at the <SpriteLayer>'s own props. Drag a
// sprite to move it - the sprite stops its down, which claims the press,
// so the <Camera2d> at the root never pans under it; drag empty space to
// pan, wheel to zoom. A tap on a sprite cycles its tint (onTap: the
// dispatch's own click, no slop bookkeeping in app code); shift-tap
// removes the sprite, exercising slot recycling (the freed pose slot zeroes
// and waits for the next add); structure lives in a signal, so <For>
// unmounts the removed <Sprite>. A tap on empty space reaches the layer
// with e.sprite null. The layer FILLS the window (no width/height): layer
// pixels are the leaf's own coordinates. A corner <View2d> renders the
// same sprites again under a <Camera2d> of its own: drag or wheel there
// moves only the inset's camera, while a sprite tapped or dragged in the
// inset responds exactly as in the main view (the same handlers, the
// view's camera undone). The `state` debug command returns both camera
// poses and every sprite's world position; `maxZoom` sets the main
// <Camera2d maxZoom> prop live (a tighter range re-clamps the pose at
// once - the props are live, only the pose props are initial values).
import { createInputMap, createPointerFeed, createSignal, decodeImage, render, For } from "@solidrt/core"
import { Camera2d, camera2dActions, camera2dBindings, createAtlas, grid, setSprite, Sprite, SpriteLayer, View2d } from "@solidrt/2d"
import type { Camera2dHandle, Frame, SpriteHandle } from "@solidrt/2d"
import { registerDebug } from "srt:dev"
import logoBytes from "./logo.png" with { type: "binary" }

const TINTS: [number, number, number, number][] = [
  [1, 1, 1, 1],
  [1, 0.5, 0.5, 1],
  [0.5, 1, 0.6, 1],
  [0.55, 0.7, 1, 1],
]
const MAX_ZOOM = 4
// The inset: view pixels, its margin from the window's corner, and the
// zoom that shows the four authored sprites whole inside it.
const INSET_WIDTH = 240
const INSET_HEIGHT = 160
const INSET_MARGIN = 16
const INSET_ZOOM = 0.25

type Item = { id: number; x: number; y: number; frame: Frame }

let cam: Camera2dHandle | undefined
let insetCam: Camera2dHandle | undefined
let handles = new Map<number, SpriteHandle>()
let [maxZoom, setMaxZoom] = createSignal(MAX_ZOOM)

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let frames = grid(atlas, 2, 2)
  let [items, setItems] = createSignal<Item[]>([
    { id: 0, x: 200, y: 240, frame: frames[0]! },
    { id: 1, x: 420, y: 300, frame: frames[1]! },
    { id: 2, x: 300, y: 470, frame: frames[2]! },
    { id: 3, x: 520, y: 500, frame: frames[3]! },
  ])
  // One pointer feed and map per view: the layer's own view and the inset
  // each drive their <Camera2d> from their own gestures and no other's.
  let pointer = createPointerFeed()
  let input = createInputMap(camera2dActions)
  input.bind(camera2dBindings({ pointer }))
  let insetPointer = createPointerFeed()
  let insetInput = createInputMap(camera2dActions)
  insetInput.bind(camera2dBindings({ pointer: insetPointer }))

  return (
    <window>
      <SpriteLayer
        atlas={atlas.texture}
        capacity={64}
        clearColor={[0.05, 0.05, 0.09, 1]}
        pointer={pointer}
        onTap={e => {
          if (e.sprite === null) console.log(`tap on empty space at ${e.x.toFixed(0)}, ${e.y.toFixed(0)} (x${e.tapCount})`)
        }}
      >
        {/* A top-left pivot keeps world (0,0) at the leaf's corner, so the
            authored positions read as window coordinates until panned. */}
        <Camera2d input={input} maxZoom={maxZoom()} pivot={{ x: 0, y: 0 }} ref={c => (cam = c)} />
        <For each={items()}>
          {item => {
            let tintIndex = 0
            let handle: SpriteHandle | undefined
            // Whether this sprite holds a press: onPointerMove also fires
            // on plain hover, and only a captured move drags.
            let pressed = false
            return (
              <Sprite
                ref={s => {
                  handle = s
                  handles.set(item.id, s)
                }}
                x={item.x}
                y={item.y}
                w={96}
                h={96}
                frame={item.frame}
                // Claim the press: the camera never sees this pointer, so
                // the captured moves drag the sprite, not the view.
                onPointerDown={e => {
                  e.stopPropagation()
                  pressed = true
                }}
                onPointerUp={() => (pressed = false)}
                onPointerMove={e => {
                  if (pressed && handle) setSprite(handle, { x: e.x, y: e.y })
                }}
                onTap={e => {
                  if (e.shiftKey) {
                    setItems(items().filter(other => other.id !== item.id))
                    return
                  }
                  tintIndex = (tintIndex + 1) % TINTS.length
                  if (handle) setSprite(handle, { tint: TINTS[tintIndex] })
                }}
              />
            )
          }}
        </For>
        {/* A laid-out element among the layer's children renders where it
            sits; the <View2d> inside stays a layer child through context. */}
        <view position="absolute" right={INSET_MARGIN} bottom={INSET_MARGIN}>
          <View2d width={INSET_WIDTH} height={INSET_HEIGHT} clearColor={[0.12, 0.1, 0.16, 1]} label="inset" pointer={insetPointer}>
            <Camera2d input={insetInput} zoom={INSET_ZOOM} maxZoom={MAX_ZOOM} pivot={{ x: 0, y: 0 }} ref={c => (insetCam = c)} />
          </View2d>
        </view>
      </SpriteLayer>
    </window>
  )
}

render(() => <App />)

registerDebug("maxZoom", (args?: { value?: number }) => {
  if (typeof args?.value === "number") setMaxZoom(args.value)
  return maxZoom()
})

registerDebug("state", () => ({
  camera: cam?.camera() ?? null,
  inset: insetCam?.camera() ?? null,
  items: [...handles].filter(([, s]) => s.layer !== null).map(([id, s]) => ({ id, x: s._x, y: s._y })),
}))
