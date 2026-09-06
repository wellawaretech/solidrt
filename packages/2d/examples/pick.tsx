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
// pixels are the leaf's own coordinates. The `state` debug command
// returns the camera pose and every sprite's world position.
import { createSignal, render, For } from "@solidrt/core"
import { Camera2d, createAtlas, grid, setSprite, Sprite, SpriteLayer } from "@solidrt/2d"
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

type Item = { id: number; x: number; y: number; frame: Frame }

let cam: Camera2dHandle | undefined
let handles = new Map<number, SpriteHandle>()

function App() {
  let atlas = createAtlas(logoBytes, { label: "logo-atlas" })
  let frames = grid(2, 2, { width: atlas.width, height: atlas.height })
  let [items, setItems] = createSignal<Item[]>([
    { id: 0, x: 200, y: 240, frame: frames[0]! },
    { id: 1, x: 420, y: 300, frame: frames[1]! },
    { id: 2, x: 300, y: 470, frame: frames[2]! },
    { id: 3, x: 520, y: 500, frame: frames[3]! },
  ])

  return (
    <window>
      <SpriteLayer
        atlas={atlas.texture}
        capacity={64}
        clearColor={[0.05, 0.05, 0.09, 1]}
        onTap={e => {
          if (e.sprite === null) console.log(`tap on empty space at ${e.x.toFixed(0)}, ${e.y.toFixed(0)} (x${e.tapCount})`)
        }}
      >
        {/* A top-left pivot keeps world (0,0) at the leaf's corner, so the
            authored positions read as window coordinates until panned. */}
        <Camera2d maxZoom={MAX_ZOOM} pivot={{ x: 0, y: 0 }} ref={c => (cam = c)} />
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
      </SpriteLayer>
    </window>
  )
}

render(() => <App />)

registerDebug("state", () => ({
  camera: cam?.camera() ?? null,
  items: [...handles].filter(([, s]) => s.layer !== null).map(([id, s]) => ({ id, x: s._x, y: s._y })),
}))
