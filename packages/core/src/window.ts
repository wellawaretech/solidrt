import { onCleanup, onSettled } from "@solidjs/signals"

// ------ Animation frames ----------------

let nextFrameId = 1
let animationFrames = new Map<number, Function>()

/**
 * Calls `fn` on every rendered frame. Returns a cleanup function to stop rendering.
 * When called within a reactive scope (e.g. a component or createEffect), cleanup is also automatic.
 */
export function onRender(fn: (tick: number) => void) {
  let frameId: number = null!

  let extendedFn = (tick: number) => {
    fn(tick)
    frameId = nextFrameId++
    animationFrames.set(frameId, extendedFn)
  }

  frameId = nextFrameId++
  animationFrames.set(frameId, extendedFn)

  let cleanup = () => animationFrames.delete(frameId)
  onCleanup(cleanup)
  return cleanup
}

// ------ Window ----------------

export function attachWindow(_nodeId: number) {
  let unsubscribe: () => void = null!

  onSettled(() => {
    unsubscribe = Flux.on("render", (time: number) => {
      if (animationFrames.size > 0) {
        let frames = animationFrames
        animationFrames = new Map()

        let t = (time * 1000) | 0
        for (let fn of frames.values()) fn(t)
      }

      draw()
    })

    // trigger first draw
    draw()
  })

  onCleanup(() => {
    if (unsubscribe) unsubscribe()
  })
}
