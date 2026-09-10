// Native node transitions at the sprite layer's scale: every sprite
// declares a spring, and the app writes TARGETS - one setSprite per sprite
// per shuffle (about once a second), ZERO JS per frame. The core
// integrates all the springs each frame and publishes one coalesced
// pose-buffer write; between shuffles the app runs no code at all (there
// is no onFrame here - the running tracks drive frame demand themselves,
// and once everything settles the layer costs nothing). Compare
// sprites.tsx, which moves the same population imperatively every frame:
// here the JS cost is proportional to target CHANGES, not frames. The
// console logs each shuffle's burst cost.
//
// Rotation targets ride the quaternion geodesic, so a spin to a new
// random angle always takes the short arc, and the position spring keeps
// its velocity when a shuffle lands mid-flight - retarget as fast as you
// like, the motion stays continuous.
//
// Tap a sprite and it LEAVES: destroySprite lets go of the handle, and the
// `exit` on its scale entry shrinks it away on an ease-in of its own while
// the sprite is a ghost - drawn, but no tap or pick reaches it - then the
// core frees it. A fresh sprite pops into the empty slot a moment later,
// entering from scale zero (`from`); no settle callback, no bookkeeping.
import { decodeImage, render } from "@solidrt/core"
import { addSprite, createAtlas, createSpriteLayer, destroySprite, grid, setSprite, setSpriteTransition } from "@solidrt/2d"
import type { SpriteHandle, SpriteTransition } from "@solidrt/2d"
import logoBytes from "./logo.png" with { type: "binary" }

const COLS = 20
const ROWS = 20
const COUNT = COLS * ROWS
const W = 720
const H = 720
const SPRITE = 30
// The pop-in / shrink-out of a sprite's size, and the pause before a
// tapped-away sprite is replaced.
const POP_MS = 300
const RESPAWN_MS = 600

const TRANSITION: SpriteTransition = {
  position: { duration: 700, bounce: 0.3 },
  rotation: { duration: 700 },
  scale: { duration: POP_MS, bounce: 0.4, from: [0, 0], exit: { value: [0, 0], curve: "ease-in", duration: POP_MS / 2 } },
}

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let frames = grid(atlas, 2, 2)
  let layer = createSpriteLayer(atlas.texture, { capacity: COUNT, label: "springs" })
  let view = layer.createView({ width: W, height: H, clearColor: [0.05, 0.05, 0.09, 1] })

  let slotX = (slot: number) => ((slot % COLS) + 0.5) * (W / COLS)
  let slotY = (slot: number) => (Math.floor(slot / COLS) + 0.5) * (H / ROWS)

  // Sprite k sits at grid slot slots[k]; each shuffle re-deals the slots.
  let slots = Array.from({ length: COUNT }, (_, i) => i)
  let spawn = (k: number): SpriteHandle => {
    let sprite = addSprite(layer, { x: slotX(slots[k]!), y: slotY(slots[k]!), w: SPRITE, h: SPRITE, frame: frames[k % 4] })
    // Declared in the adding tick, so the `from` plays: the sprite pops in.
    setSpriteTransition(sprite, TRANSITION)
    sprite.onTap = () => {
      destroySprite(sprite)
      setTimeout(() => (sprites[k] = spawn(k)), RESPAWN_MS)
    }
    return sprite
  }
  let sprites = slots.map((_, k) => spawn(k))

  let shuffle = () => {
    for (let i = slots.length - 1; i > 0; i--) {
      let j = Math.floor(Math.random() * (i + 1))
      ;[slots[i], slots[j]] = [slots[j]!, slots[i]!]
    }
    let start = performance.now()
    for (let k = 0; k < COUNT; k++) {
      setSprite(sprites[k]!, { x: slotX(slots[k]!), y: slotY(slots[k]!), rotation: Math.random() * Math.PI * 2 })
    }
    let ms = performance.now() - start
    console.log(`retarget x${COUNT}: ${ms.toFixed(2)} ms; JS idles until the next shuffle`)
  }
  setInterval(shuffle, 1200)

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={view.texture} width={W} height={H} {...view.handlers} />
    </window>
  )
}

render(() => <App />)
