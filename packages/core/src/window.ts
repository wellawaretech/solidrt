import { onCleanup, onSettled, flush } from "@solidjs/signals"
import { getEventHandler } from "./events"
import { getFocusedNodeId, setFocus } from "./focus"

// ------ Animation frames ----------------

let nextFrameId = 1
let animationFrames = new Map<number, Function>()

/**
 * Calls `fn` before every frame is painted. The first call receives tick=0 (game time).
 * Returns a cleanup function to stop updates. When called within a reactive scope
 * (e.g. a component or createEffect), cleanup is also automatic.
 */
export function onFrame(fn: (tick: number, frame: number) => void) {
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

// Fires after layout has been computed for the current frame but before paint.
// Setting properties that affect layout from this callback will be picked up
// by a re-layout pass before painting (one extra pass; cascades beyond that
// paint stale).
export function onLayout(fn: () => void) {
  let unsubscribe = Flux.on("postLayout", fn)
  onCleanup(unsubscribe)
  return unsubscribe
}

export function onWindowFocus(fn: () => void) {
  let unsubscribe = Flux.on("windowFocus", fn)
  onCleanup(unsubscribe)
  return unsubscribe
}

export function onWindowBlur(fn: () => void) {
  let unsubscribe = Flux.on("windowBlur", fn)
  onCleanup(unsubscribe)
  return unsubscribe
}

// ------ Window ----------------

export function attachWindow(_nodeId: number) {
  let unsubscribe: () => void = null!
  let unsubDown: () => void = null!
  let unsubUp: () => void = null!
  let unsubMove: () => void = null!
  let unsubEnter: () => void = null!
  let unsubLeave: () => void = null!
  let unsubWheel: () => void = null!
  let unsubKeyDown: () => void = null!
  let unsubKeyUp: () => void = null!
  let unsubTextInput: () => void = null!
  let unsubKeyboardVisibility: () => void = null!
  let unsubFirstResize: (() => void) | null = null

  function runFrame(t: number, frame: number) {
    if (animationFrames.size > 0) {
      let frames = animationFrames
      animationFrames = new Map()
      for (let fn of frames.values()) fn(t, frame)
    }
    flush()
    draw()
  }

  onSettled(() => {
    unsubscribe = Flux.on("render", ({ time, frame }: { time: number; frame: number }) => {
      runFrame((time * 1000) | 0, frame)
    })

    unsubDown = Flux.on(
      "pointerDown",
      ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
        for (let nodeId of targets) {
          getEventHandler(nodeId, "onPointerDown")?.(e)
        }
        // Outside-tap blur. Read focus AFTER per-node handlers so a tap that
        // moves focus to a new node is not immediately blurred again.
        let focused = getFocusedNodeId()
        if (focused != null && !targets.includes(focused)) {
          setFocus(null)
        }
      },
    )

    unsubUp = Flux.on("pointerUp", ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onPointerUp")?.(e)
      }
    })

    unsubMove = Flux.on(
      "pointerMove",
      ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
        for (let nodeId of targets) {
          getEventHandler(nodeId, "onPointerMove")?.(e)
        }
      },
    )

    unsubEnter = Flux.on(
      "pointerEnter",
      ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
        for (let nodeId of targets) {
          getEventHandler(nodeId, "onPointerEnter")?.(e)
        }
      },
    )

    unsubLeave = Flux.on(
      "pointerLeave",
      ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
        for (let nodeId of targets) {
          getEventHandler(nodeId, "onPointerLeave")?.(e)
        }
      },
    )

    unsubWheel = Flux.on("wheel", ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
      for (let nodeId of targets) {
        getEventHandler(nodeId, "onWheel")?.(e)
      }
    })

    unsubKeyDown = Flux.on("keydown", (e: any) => {
      let id = getFocusedNodeId()
      if (id != null) {
        getEventHandler(id, "onKeyDown")?.(e)
      }
    })

    unsubKeyUp = Flux.on("keyup", (e: any) => {
      let id = getFocusedNodeId()
      if (id != null) {
        getEventHandler(id, "onKeyUp")?.(e)
      }
    })

    unsubTextInput = Flux.on("textInput", (e: any) => {
      let id = getFocusedNodeId()
      if (id != null) {
        getEventHandler(id, "onTextInput")?.(e)
      }
    })

    // When the user dismisses the on-screen keyboard (swipe down, "Done",
    // back button), blur the focused node so the app's UI state catches up.
    unsubKeyboardVisibility = Flux.on("keyboardVisibility", ({ shown }: { shown: boolean }) => {
      if (!shown) setFocus(null)
    })

    // Bootstrap the first frame on the first resize event: by then any
    // onResize subscribers (which run earlier in the dispatch list) have
    // set their initial signal values, so runFrame's flush sees a fully
    // initialized graph. Resize is a sticky event in Flux, so this fires
    // synchronously here if a value has already been cached. Runs outside
    // the tracked-effect scope so flush() is legal.
    unsubFirstResize = Flux.once("resize", () => runFrame(0, 0))
  })

  onCleanup(() => {
    if (unsubscribe) unsubscribe()
    if (unsubDown) unsubDown()
    if (unsubUp) unsubUp()
    if (unsubMove) unsubMove()
    if (unsubEnter) unsubEnter()
    if (unsubLeave) unsubLeave()
    if (unsubWheel) unsubWheel()
    if (unsubKeyDown) unsubKeyDown()
    if (unsubKeyUp) unsubKeyUp()
    if (unsubTextInput) unsubTextInput()
    if (unsubKeyboardVisibility) unsubKeyboardVisibility()
    if (unsubFirstResize) unsubFirstResize()
  })
}
