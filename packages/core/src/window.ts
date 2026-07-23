import { createSignal, onCleanup, onSettled, flush } from "@solidjs/signals"
import { requestFrame } from "flux:rendertree"
import { renderFrame } from "srt:render"
import { on, once } from "srt:events"
import { getEventHandler, getFocusedNodeId, setFocus } from "./core"
import { scanForOrphans } from "./renderer"

// ------ Pointer routing -----------------

// Hit path frozen at pointerDown, pointerId -> targets. While a pointer has an
// active down, its moves and up dispatch along this path (same leaf-to-root
// bubble) no matter where the pointer currently is, so every node under the
// original down observes the whole gesture: a drag keeps working off-element,
// and an ancestor recognizer (a scroller's pan) sees the moves it needs to
// take over mid-gesture via the arena. There is no exclusive pointer capture;
// gesture ownership is claim-based, above this layer. Enter/leave stay
// hover-driven, and moves with no active down follow the live hit path.
let downPaths = new Map<number, number[]>()

// ------ Animation frames ----------------

let nextFrameId = 1
let animationFrames = new Map<number, Function>()

// Latest display refresh rate (Hz) reported by the runtime, passed to onFrame
// callbacks. Defaults to 60 until the first displayRefreshRate event arrives.
let refreshRate = 60

/**
 * Calls `fn` before every frame is painted: `tick` is the time in ms, `frame` is
 * the present count, and `rate` is the current refresh rate in Hz. `tick` is paced
 * by the runtime (one refresh period per present, slow-corrected toward the wall
 * clock) so animations driven off it stay smooth even when swap-return times
 * jitter. For raw, continuous wall-clock time (e.g. measuring code), use
 * performance.now() instead.
 * Returns a cleanup function; also auto-cleans within a reactive scope.
 */
export function onFrame(fn: (tick: number, frame: number, rate: number) => void) {
  let frameId: number = null!

  let extendedFn = (tick: number, frame: number, rate: number) => {
    fn(tick, frame, rate)
    frameId = nextFrameId++
    animationFrames.set(frameId, extendedFn)
    // A pending onFrame callback is a standing request for the next frame.
    requestFrame()
  }

  frameId = nextFrameId++
  animationFrames.set(frameId, extendedFn)
  requestFrame()

  let cleanup = () => animationFrames.delete(frameId)
  onCleanup(cleanup)
  return cleanup
}

// ------ Resize ----------------

// Insets: each value is the distance from the corresponding window edge, like
// CSS env(safe-area-inset-*).
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
  let unsubscribe = on("resize", fn)
  onCleanup(unsubscribe)
  return unsubscribe
}

// ------ Reactive window state ----------------

// Singleton accessors over the same events as onResize / onWindowFocus. There
// is one window, so these are bare accessors rather than a createX instance.
// Lazily subscribed on first read (resize is sticky, so the first read sees the
// current value); app-lifetime, so no onCleanup. `ownedWrite: true` on the
// signals below is the sticky-replay-into-a-tracked-scope case explained on
// the ensure* functions in environment.ts.

let sizeAccessor: (() => { width: number; height: number }) | undefined
let safeAreaAccessor: (() => SafeArea) | undefined
let displayScaleAccessor: (() => number) | undefined

function ensureResizeState() {
  if (sizeAccessor) return
  let [size, setSize] = createSignal({ width: 0, height: 0 }, { ownedWrite: true })
  let [safe, setSafe] = createSignal<SafeArea>({ top: 0, left: 0, right: 0, bottom: 0 }, { ownedWrite: true })
  let [scale, setScale] = createSignal(1, { ownedWrite: true })
  on("resize", (e: ResizeEvent) => {
    setSize({ width: e.width, height: e.height })
    setSafe(e.safeArea)
    setScale(e.displayScale)
  })
  sizeAccessor = size
  safeAreaAccessor = safe
  displayScaleAccessor = scale
}

/** Current window size, as a reactive accessor. */
export function windowSize(): { width: number; height: number } {
  ensureResizeState()
  return sizeAccessor!()
}

/** Current safe-area insets, as a reactive accessor. */
export function safeArea(): SafeArea {
  ensureResizeState()
  return safeAreaAccessor!()
}

/** Current display scale (device pixel ratio), as a reactive accessor. */
export function displayScale(): number {
  ensureResizeState()
  return displayScaleAccessor!()
}

let focusedAccessor: (() => boolean) | undefined

/** Whether the window currently has focus, as a reactive accessor. */
export function windowFocused(): boolean {
  if (!focusedAccessor) {
    let [focused, setFocused] = createSignal(true)
    on("windowFocus", () => setFocused(true))
    on("windowBlur", () => setFocused(false))
    focusedAccessor = focused
  }
  return focusedAccessor()
}

let keyboardHeightAccessor: (() => number) | undefined

/**
 * Height in logical pixels that the on-screen keyboard overlaps the window
 * (0 when hidden or on platforms without a soft keyboard), as a reactive
 * accessor. The window is not resized for the keyboard, so pad or lift content
 * by this much to keep it above the keyboard.
 */
export function keyboardHeight(): number {
  if (!keyboardHeightAccessor) {
    let [height, setHeight] = createSignal(0)
    on("keyboardVisibility", ({ height: h }: { height: number }) => setHeight(h ?? 0))
    keyboardHeightAccessor = height
  }
  return keyboardHeightAccessor()
}

/**
 * Fires after layout has been computed for the current frame but before paint.
 * Setting properties that affect layout from this callback will be picked up
 * by a re-layout pass before painting (one extra pass; cascades beyond that
 * paint stale).
 */
export function onLayout(fn: () => void) {
  let unsubscribe = on("postLayout", fn)
  onCleanup(unsubscribe)
  return unsubscribe
}

export function onWindowFocus(fn: () => void) {
  let unsubscribe = on("windowFocus", fn)
  onCleanup(unsubscribe)
  return unsubscribe
}

export function onWindowBlur(fn: () => void) {
  let unsubscribe = on("windowBlur", fn)
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
    scanForOrphans(t)
    renderFrame()
  }

  onSettled(() => {
    // Sticky event: a late subscriber still receives the current rate.
    unsubRefreshRate = on("displayRefreshRate", ({ hz }: { hz: number }) => {
      if (hz > 0) refreshRate = hz
    })

    unsubscribe = on("render", ({ time, frame }: { time: number; frame: number }) => {
      runFrame(time * 1000, frame)
    })

    // Dispatch an event to every node on the hit path, leaf->root (bubbling), so
    // a child handler can call e.stopPropagation() to keep the event from
    // reaching its ancestors. `targets` arrives root->leaf, hence the reverse.
    let bubble = (targets: number[], handler: string, e: any) => {
      let stopped = false
      e.stopPropagation = () => {
        stopped = true
      }
      for (let i = targets.length - 1; i >= 0; i--) {
        getEventHandler(targets[i]!, handler)?.(e)
        if (stopped) break
      }
    }

    unsubDown = on(
      "pointerDown",
      ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
        downPaths.set(e.pointerId, targets)
        bubble(targets, "onPointerDown", e)
        // Outside-tap blur. Read focus AFTER per-node handlers so a tap that
        // moves focus to a new node is not immediately blurred again.
        let focused = getFocusedNodeId()
        if (focused != null && !targets.includes(focused)) {
          setFocus(null)
        }
      },
    )

    unsubUp = on("pointerUp", ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
      let frozen = downPaths.get(e.pointerId)
      downPaths.delete(e.pointerId)
      bubble(frozen ?? targets, "onPointerUp", e)
    })

    unsubMove = on(
      "pointerMove",
      ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
        bubble(downPaths.get(e.pointerId) ?? targets, "onPointerMove", e)
      },
    )

    // Enter/leave keep their hover-diff order (already leaf->root for leave,
    // root->leaf for enter), but still honor stopPropagation for a consistent
    // event shape.
    let dispatchOrdered = (targets: number[], handler: string, e: any) => {
      let stopped = false
      e.stopPropagation = () => {
        stopped = true
      }
      for (let nodeId of targets) {
        getEventHandler(nodeId, handler)?.(e)
        if (stopped) break
      }
    }

    unsubEnter = on(
      "pointerEnter",
      ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
        dispatchOrdered(targets, "onPointerEnter", e)
      },
    )

    unsubLeave = on(
      "pointerLeave",
      ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
        dispatchOrdered(targets, "onPointerLeave", e)
      },
    )

    unsubWheel = on("wheel", ({ targets, ...e }: { targets: number[]; [k: string]: any }) => {
      bubble(targets, "onWheel", e)
    })

    unsubKeyDown = on("keydown", (e: any) => {
      let id = getFocusedNodeId()
      if (id != null) {
        getEventHandler(id, "onKeyDown")?.(e)
      }
    })

    unsubKeyUp = on("keyup", (e: any) => {
      let id = getFocusedNodeId()
      if (id != null) {
        getEventHandler(id, "onKeyUp")?.(e)
      }
    })

    unsubTextInput = on("textInput", (e: any) => {
      let id = getFocusedNodeId()
      if (id != null) {
        getEventHandler(id, "onTextInput")?.(e)
      }
    })

    // When the user dismisses the on-screen keyboard (swipe down, "Done",
    // back button), blur the focused node so the app's UI state catches up.
    unsubKeyboardVisibility = on("keyboardVisibility", ({ shown }: { shown: boolean }) => {
      if (!shown) setFocus(null)
    })

    // Bootstrap the first frame on the first resize event: by then any
    // onResize subscribers (which run earlier in the dispatch list) have
    // set their initial signal values, so runFrame's flush sees a fully
    // initialized graph. Resize is a sticky event in Flux, so it can replay
    // synchronously here, while we are still inside this onSettled callback
    // where flush() is illegal (not reentrant). Defer runFrame to a microtask
    // so the first frame always runs after this callback returns.
    unsubFirstResize = once("resize", () => {
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
