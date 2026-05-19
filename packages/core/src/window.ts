import { onCleanup, onSettled } from "@solidjs/signals"
import { getEventHandler } from "./events"

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
  let unsubDown: () => void = null!
  let unsubEnter: () => void = null!
  let unsubLeave: () => void = null!

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

    unsubDown = Flux.on("pointerDown", ({ targets, button, clientX, clientY }: { targets: number[], button: number, clientX: number, clientY: number }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerDown")?.({ button, clientX, clientY })
      }
    })

    unsubEnter = Flux.on("pointerEnter", ({ targets }: { targets: number[] }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerEnter")?.()
      }
    })

    unsubLeave = Flux.on("pointerLeave", ({ targets }: { targets: number[] }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerLeave")?.()
      }
    })

    // trigger first draw
    draw()
  })

  onCleanup(() => {
    if (unsubscribe) unsubscribe()
    if (unsubDown) unsubDown()
    if (unsubEnter) unsubEnter()
    if (unsubLeave) unsubLeave()
  })
}
