import { onCleanup, onSettled } from "@solidjs/signals"

// ------ Animation frames ----------------

let nextFrameId = 1
let animationFrames = new Map<number, Function>()

/**
 * Calls `fn` on every rendered frame. Returns a cleanup function to stop rendering.
 * When called within a reactive scope (e.g. a component or createEffect), cleanup is also automatic.
 */
export function onRender(fn: (tick: number, frame: number) => void) {
  let frameId: number = null!

  let extendedFn = (tick: number, frame: number) => {
    fn(tick, frame)
    frameId = nextFrameId++
    animationFrames.set(frameId, extendedFn)
  }

  frameId = nextFrameId++
  animationFrames.set(frameId, extendedFn)

  let cleanup = () => animationFrames.delete(frameId)
  onCleanup(cleanup)
  return cleanup
}

// ------ Resize ----------------

interface SafeArea {
  top: number
  left: number
  right: number
  bottom: number
}

interface ResizeEvent {
  width: number
  height: number
  safeArea: SafeArea
  displayScale: number
}

export function onResize(fn: (data: ResizeEvent) => void) {
  let unsubscribe = Flux.on("resize", fn)
  onCleanup(unsubscribe)
  return unsubscribe
}

// ------ Window ----------------

export function attachWindow(_nodeId: number) {
  let unsubscribe: () => void = null!

  onSettled(() => {
    unsubscribe = Flux.on("render", ({ time, frame }: { time: number, frame: number }) => {
      if (animationFrames.size > 0) {
        let frames = animationFrames
        animationFrames = new Map()

        let t = (time * 1000) | 0
        for (let fn of frames.values()) fn(t, frame)
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
