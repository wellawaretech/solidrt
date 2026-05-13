import { onCleanup, onSettled } from "@solidjs/signals"

// ------ Animation frames ----------------

let nextFrameId = 1
let animationFrames = new Map<number, Function>()

export let requestAnimationFrame = (fn: (tick: number) => void) => {
  let id = nextFrameId++
  animationFrames.set(id, fn)
  return id
}

export let cancelAnimationFrame = (id: number) => {
  animationFrames.delete(id)
}

export function onRender(fn: (tick: number) => void) {
  let frameId: number = null!

  let extendedFn = (tick: number) => {
    fn(tick)
    frameId = requestAnimationFrame(extendedFn)
  }

  frameId = requestAnimationFrame(extendedFn)
  onCleanup(() => cancelAnimationFrame(frameId))
}

// ------ Window ----------------

export function attachWindow(_nodeId: number) {
  let unsubscribe: (() => void) | null = null

  onSettled(() => {
    unsubscribe = Flux.on("render", (time: number) => {
      if (animationFrames.size > 0) {
        let frames = animationFrames
        animationFrames = new Map()

        let t = time * 1000 | 0
        for (let fn of frames.values()) {
          fn(t)
        }
      }

      draw()
    })
  })

  onCleanup(() => {
    if (unsubscribe) unsubscribe()
  })
}
