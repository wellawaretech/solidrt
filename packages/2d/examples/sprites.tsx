// The sprite layer at its natural scale: hundreds of sprites bouncing at
// frame rate, driven imperatively. The component face mounts the layer; the
// motion loop grabs sprite handles via addSprite and rewrites positions with
// setSprite from onFrame - signals carry structure, per-frame motion goes
// straight to the layer (the same split as @solidrt/3d). Whatever moves, the
// tree holds ONE texture leaf; each setSprite writes the sprite's arena
// node, and the core flush publishes every moved pose as one coalesced
// instance-buffer write per frame.
//
// The atlas is a real image (core's logo) sliced 2x2 by grid(): four frames,
// each sprite drawing one quarter. Pixels built in code go to createAtlas as
// they are; this path decodes PNG bytes imported with { type: "binary" }
// first.
import { decodeImage, onFrame, render } from "@solidrt/core"
import { addSprite, createAtlas, createSpriteLayer, grid, setSprite } from "@solidrt/2d"
import logoBytes from "./logo.png" with { type: "binary" }

const COUNT = 500
const W = 720
const H = 720
const SPRITE = 48

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let frames = grid(atlas, 2, 2)
  let layer = createSpriteLayer(atlas.texture, { capacity: COUNT, label: "bounce" })
  let view = layer.createView({ width: W, height: H, clearColor: [0.05, 0.05, 0.09, 1] })

  // Simulation state lives in plain arrays; the layer holds the published
  // snapshot of it.
  let x = new Float32Array(COUNT)
  let y = new Float32Array(COUNT)
  let vx = new Float32Array(COUNT)
  let vy = new Float32Array(COUNT)
  let sprites = new Array(COUNT)
  for (let i = 0; i < COUNT; i++) {
    x[i] = SPRITE / 2 + Math.random() * (W - SPRITE)
    y[i] = SPRITE / 2 + Math.random() * (H - SPRITE)
    vx[i] = (Math.random() * 2 - 1) * 3
    vy[i] = (Math.random() * 2 - 1) * 3
    sprites[i] = addSprite(layer, {
      x: x[i],
      y: y[i],
      w: SPRITE,
      h: SPRITE,
      frame: frames[i % 4],
      rotation: Math.random() * Math.PI * 2,
      // A sprite faces its motion: mirrored on the UV side, so the flip is
      // free of the pose and instant under any transition.
      flipX: vx[i]! < 0,
      flipY: vy[i]! < 0,
    })
  }

  onFrame(tick => {
    for (let i = 0; i < COUNT; i++) {
      let nx = x[i]! + vx[i]!
      let ny = y[i]! + vy[i]!
      if (nx < SPRITE / 2 || nx > W - SPRITE / 2) {
        vx[i] = -vx[i]!
        setSprite(sprites[i]!, { flipX: vx[i]! < 0 })
      } else x[i] = nx
      if (ny < SPRITE / 2 || ny > H - SPRITE / 2) {
        vy[i] = -vy[i]!
        setSprite(sprites[i]!, { flipY: vy[i]! < 0 })
      } else y[i] = ny
      setSprite(sprites[i]!, { x: x[i]!, y: y[i]!, rotation: tick / 1000 + i })
    }
  })

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={view.texture} width={W} height={H} />
    </window>
  )
}

render(() => <App />)
