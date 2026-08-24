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
import { render } from "@solidrt/core"
import { addSprite, createAtlas, createSpriteLayer, grid, setSprite, setSpriteTransition } from "@solidrt/2d"
import logoBytes from "./logo.png" with { type: "binary" }

const COLS = 20
const ROWS = 20
const COUNT = COLS * ROWS
const W = 720
const H = 720
const SPRITE = 30

function App() {
  let atlas = createAtlas(logoBytes, { label: "logo-atlas" })
  let frames = grid(2, 2, { width: atlas.width, height: atlas.height })
  let layer = createSpriteLayer(W, H, atlas.texture, {
    capacity: COUNT,
    clearColor: [0.05, 0.05, 0.09, 1],
    label: "springs",
  })

  let slotX = (slot: number) => ((slot % COLS) + 0.5) * (W / COLS)
  let slotY = (slot: number) => (Math.floor(slot / COLS) + 0.5) * (H / ROWS)

  // Sprite k sits at grid slot slots[k]; each shuffle re-deals the slots.
  let slots = Array.from({ length: COUNT }, (_, i) => i)
  let sprites = slots.map(slot =>
    addSprite(layer, { x: slotX(slot), y: slotY(slot), w: SPRITE, h: SPRITE, frame: frames[slot % 4] }),
  )
  for (let sprite of sprites) {
    setSpriteTransition(sprite, { position: { duration: 700, bounce: 0.3 }, rotation: { duration: 700 } })
  }

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
      <texture src={layer.texture} width={W} height={H} />
    </window>
  )
}

render(() => <App />)
