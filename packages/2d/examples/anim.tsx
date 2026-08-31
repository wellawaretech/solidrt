// Frame animation via createAnimation: a row of sprites sharing one looping
// clip (one clock, one setSprite per sprite per STEP - an app's whole crowd
// animates off a handful of timers, no onFrame loop), plus a one-shot clip
// that holds its last frame and fires onEnd. Self-asserting: samples the
// sprites' frames against the wall clock and logs ANIM-OK / ANIM-FAIL.
//
// The atlas is the core logo sliced 2x2 by grid(): a 4-frame "clip". Real
// art would slice a sheet the same way and pass a consecutive slice of the
// frames array per animation.
import { render } from "@solidrt/core"
import { addSprite, createAnimation, createAtlas, createSpriteLayer, getSprite, grid } from "@solidrt/2d"
import type { Frame, SpriteHandle } from "@solidrt/2d"
import logoBytes from "./logo.png" with { type: "binary" }

const W = 520
const H = 200
const SPRITE = 96
// Slow enough that timer jitter cannot blur a sampled frame boundary: the
// checks sample mid-frame, half a period from either edge.
const LOOP_FPS = 2
const SHOT_FPS = 5

function App() {
  let atlas = createAtlas(logoBytes, { label: "logo-atlas" })
  let frames = grid(2, 2, { width: atlas.width, height: atlas.height })
  let layer = createSpriteLayer(W, H, atlas.texture, {
    clearColor: [0.05, 0.05, 0.09, 1],
    label: "anim",
  })

  // Three sprites share the looping clip - one clock steps all of them.
  let looping = createAnimation(frames, LOOP_FPS)
  let looped: SpriteHandle[] = []
  for (let i = 0; i < 3; i++) {
    let sprite = addSprite(layer, { x: 80 + i * 120, y: H / 2, w: SPRITE, h: SPRITE })
    looping.add(sprite)
    looped.push(sprite)
  }

  // The one-shot: plays through once, holds the last frame, fires onEnd.
  let shot = createAnimation(frames, SHOT_FPS, { loop: false })
  let ended = 0
  shot.onEnd = () => ended++
  let shotSprite = addSprite(layer, { x: 440, y: H / 2, w: SPRITE, h: SPRITE })
  shot.add(shotSprite)

  let failures: string[] = []
  function check(ok: boolean, what: string) {
    if (!ok) failures.push(what)
  }
  function indexOf(frame: Frame): number {
    return frames.findIndex(f => Math.abs(f.u0 - frame.u0) < 1e-6 && Math.abs(f.v0 - frame.v0) < 1e-6)
  }
  function shownIndex(sprite: SpriteHandle): number {
    return indexOf(getSprite(sprite)!.frame)
  }

  // Sample mid-frame against the wall clock. At LOOP_FPS = 2 a frame is
  // 500ms; the clip steps within ~250ms of a boundary (tick at half a
  // period), so mid-frame samples have ~250ms of margin either way.
  let period = 1000 / LOOP_FPS
  setTimeout(() => {
    check(shownIndex(looped[0]!) === 0, "loop shows frame 0 in its first period")
  }, period / 2)
  setTimeout(() => {
    check(shownIndex(looped[1]!) === 1, "loop advanced to frame 1")
    check(looping.frame === shownIndex(looped[1]!), "anim.frame matches the sprite's shown frame")
  }, period * 1.5)
  setTimeout(() => {
    check(shownIndex(looped[2]!) === 0, "loop wrapped back to frame 0 after a full cycle")
    // One-shot: 4 frames at 5fps end at 800ms - long since done here.
    check(shownIndex(shotSprite) === frames.length - 1, "one-shot holds its last frame")
    check(!shot.playing, "one-shot stopped playing")
    check(ended === 1, `onEnd fired once, got ${ended}`)
    check(looping.playing, "the looping clip is still playing")
    console.log(failures.length === 0 ? "ANIM-OK" : `ANIM-FAIL: ${failures.join("; ")}`)
  }, period * 4.5)

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={layer.texture} width={W} height={H} />
    </window>
  )
}

render(() => <App />)
