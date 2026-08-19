// Sprite pointer events through the component face: the built-in leaf
// carries the layer's handlers, so <Sprite onPointer*> props receive events
// with LAYER-pixel coordinates and exact rotated-rect hit testing, topmost
// sprite first. Drag a sprite to move it - the layer captures the pointer on
// down, so the drag keeps delivering to the grabbed sprite even when the
// pointer outruns it. A click (press and release without moving) cycles the
// tint. Shift-click removes the sprite, exercising the order-preserving
// record shift; structure lives in a signal, so <For> unmounts the removed
// <Sprite> and the layer compacts.
import { createSignal, render, For } from "@solidrt/core"
import { createAtlas, grid, setSprite, Sprite, SpriteLayer } from "@solidrt/2d"
import type { Frame, SpriteHandle } from "@solidrt/2d"
import logoBytes from "./logo.png" with { type: "binary" }

const W = 720
const H = 720

const TINTS: [number, number, number, number][] = [
  [1, 1, 1, 1],
  [1, 0.5, 0.5, 1],
  [0.5, 1, 0.6, 1],
  [0.55, 0.7, 1, 1],
]

type Item = { id: number; x: number; y: number; frame: Frame }

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
    <window alignItems="center" justifyContent="center">
      <SpriteLayer width={W} height={H} atlas={atlas.texture} capacity={64} clearColor={[0.05, 0.05, 0.09, 1]}>
        <For each={items()}>
          {item => {
            let tintIndex = 0
            let down = false
            let moved = false
            let downX = 0
            let downY = 0
            let handle: SpriteHandle | undefined
            return (
              <Sprite
                ref={s => (handle = s)}
                x={item.x}
                y={item.y}
                w={96}
                h={96}
                frame={item.frame}
                onPointerDown={e => {
                  down = true
                  moved = false
                  downX = e.x
                  downY = e.y
                }}
                onPointerMove={e => {
                  // onPointerMove also fires on plain hover; only a captured
                  // move (button held since the down) drags. A few pixels of
                  // slop keep an ordinary click from registering as a drag.
                  if (!down) return
                  if (!moved && Math.hypot(e.x - downX, e.y - downY) < 4) return
                  moved = true
                  if (handle) setSprite(handle, { x: e.x, y: e.y })
                }}
                onPointerUp={e => {
                  down = false
                  if (moved) return
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
