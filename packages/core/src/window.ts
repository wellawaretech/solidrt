import { onCleanup, onSettled, flush, createSignal } from "@solidjs/signals"
import { getEventHandler, getFocusedNodeId, setFocus } from "./core"

// ------ Animation frames ----------------

let nextFrameId = 1
let animationFrames = new Map<number, Function>()

// Latest display refresh rate (Hz) reported by the runtime, passed to onFrame
// callbacks. Defaults to 60 until the first displayRefreshRate event arrives.
let refreshRate = 60

/**
 * Calls `fn` before every frame is painted, with the raw runtime signals: `tick`
 * is the unsmoothed wall-clock time in ms sampled at present, `frame` is the
 * present count, and `rate` is the current refresh rate in Hz. No pacing is
 * applied; see createPacedClock for an opt-in smooth clock.
 * Returns a cleanup function; also auto-cleans within a reactive scope.
 */
export function onFrame(fn: (tick: number, frame: number, rate: number) => void) {
  let frameId: number = null!

  let extendedFn = (tick: number, frame: number, rate: number) => {
    fn(tick, frame, rate)
    frameId = nextFrameId++
    animationFrames.set(frameId, extendedFn)
  }

  frameId = nextFrameId++
  animationFrames.set(frameId, extendedFn)

  let cleanup = () => animationFrames.delete(frameId)
  onCleanup(cleanup)
  return cleanup
}

/**
 * Opt-in smooth clock built on the raw onFrame signals. Paces by present count
 * (one refresh period per frame) and slowly corrects toward the raw wall-clock
 * tick, so it stays smooth while keeping up and tracks real time when the
 * framerate drops. `gain` (0..1) trades convergence speed for jitter. Returns an
 * accessor for the paced time in milliseconds.
 */
export function createPacedClock(opts?: { gain?: number }) {
  let gain = opts?.gain ?? 0.05
  let [time, setTime] = createSignal(0)
  let clock = 0
  onFrame((tick, _frame, rate) => {
    let period = 1000 / rate
    clock += period
    clock += (tick - clock) * gain
    setTime(clock)
  })
  return time
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
  let unsubRefreshRate: () => void = null!
  let unsubFirstResize: (() => void) | null = null

  function runFrame(t: number, frame: number) {
    if (animationFrames.size > 0) {
      let frames = animationFrames
      animationFrames = new Map()
      for (let fn of frames.values()) fn(t, frame, refreshRate)
    }
    flush()
    draw()
  }

  onSettled(() => {
    // Sticky event: a late subscriber still receives the current rate.
    unsubRefreshRate = Flux.on("displayRefreshRate", ({ hz }: { hz: number }) => {
      if (hz > 0) refreshRate = hz
    })

    unsubscribe = Flux.on("render", ({ time, frame }: { time: number; frame: number }) => {
      runFrame(time * 1000, frame)
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
    // initialized graph. Resize is a sticky event in Flux, so it can replay
    // synchronously here, while we are still inside this onSettled callback
    // where flush() is illegal (not reentrant). Defer runFrame to a microtask
    // so the first frame always runs after this callback returns.
    unsubFirstResize = Flux.once("resize", () => {
      queueMicrotask(() => runFrame(0, 0))
    })
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
    if (unsubRefreshRate) unsubRefreshRate()
    if (unsubFirstResize) unsubFirstResize()
  })
}
