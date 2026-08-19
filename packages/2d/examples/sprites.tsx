// The sprite layer at its natural scale: hundreds of sprites bouncing at
// frame rate, driven imperatively. The component face mounts the layer; the
// motion loop grabs sprite handles via addSprite and rewrites positions with
// setSprite from onFrame - signals carry structure, per-frame motion goes
// straight to the layer (the same split as @solidrt/3d). Whatever moves, the
// tree holds ONE texture leaf; the runtime publishes the record buffer
// through the zero-copy write lease once per dirty frame.
//
// The atlas is a real image (core's logo) sliced 2x2 by grid(): four frames,
// each sprite drawing one quarter. An atlas from raw pixel bytes would use
// createTexture directly; this path (createAtlas) decodes PNG bytes imported
// with { type: "binary" }.
import { onFrame, render } from "@solidrt/core"
import { addSprite, createAtlas, createSpriteLayer, grid, setSprite } from "@solidrt/2d"
import logoBytes from "./logo.png" with { type: "binary" }

const COUNT = 500
const W = 720
const H = 720
const SPRITE = 48

function App() {
  let atlas = createAtlas(logoBytes, { label: "logo-atlas" })
  let frames = grid(2, 2, { width: atlas.width, height: atlas.height })
  let layer = createSpriteLayer(W, H, atlas.texture, {
    capacity: COUNT,
    clearColor: [0.05, 0.05, 0.09, 1],
    label: "bounce",
  })

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
    })
  }

  onFrame(tick => {
    for (let i = 0; i < COUNT; i++) {
      let nx = x[i]! + vx[i]!
      let ny = y[i]! + vy[i]!
      if (nx < SPRITE / 2 || nx > W - SPRITE / 2) vx[i] = -vx[i]!
      else x[i] = nx
      if (ny < SPRITE / 2 || ny > H - SPRITE / 2) vy[i] = -vy[i]!
      else y[i] = ny
      setSprite(sprites[i]!, { x: x[i]!, y: y[i]!, rotation: tick / 1000 + i })
    }
  })

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={layer.texture} width={W} height={H} />
    </window>
  )
}

render(() => <App />)
