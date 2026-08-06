import { createSignal, onCleanup, onSettled, flush } from "@solidjs/signals"
import { requestFrame } from "flux:rendertree"
import { renderFrame } from "srt:render"
import { on, once } from "srt:events"
import { exit } from "srt:app"
import { getEventHandler, focusedNode, setFocus, activateTextInput, setInterestRoot } from "./core"
import { scanForOrphans, getNodePath } from "./renderer"

/**
 * Leaves the current app, unconditionally: back to the launcher in a dev
 * client, quitting when standalone or at the launcher itself (on Android the
 * client backgrounds instead of dying). The default action of an unprevented
 * `back` event; call it directly to exit programmatically, e.g. after
 * intercepting back for an unsaved-changes dialog.
 */
export { exit }

// ------ Pointer routing -----------------

// Routing lives in the engine: the runtime freezes each pointer's hit path at
// pointerDown and delivers every event with its exact targets plus per-node
// local/parent-frame coordinate arrays (see PointerEvent in types.d.ts). This
// side only walks the delivered path, resolving the per-node scalars before
// each handler. There is no exclusive pointer capture; gesture ownership is
// claim-based, above this layer.

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
 * jitter. performance.now() and timers report/march on this same paced timeline
 * (so the whole time surface freezes together under the dev tools' clock
 * control); for real wall-clock time use Date.now().
 * Returns a cleanup function; also auto-cleans within a reactive scope.
 */
export function onFrame(fn: (tick: number, frame: number, rate: number) => void) {
  let frameId: number = null!
  // Cancellation is a flag, not map membership: while a frame runs the whole
  // callback map is swapped out, so a cleanup() called from another onFrame
  // callback in the same tick could not reach this entry through the map.
  let cancelled = false

  let extendedFn = (tick: number, frame: number, rate: number) => {
    if (cancelled) return
    // Re-register BEFORE running fn, so a throwing callback stays subscribed
    // (event-listener semantics: the error is reported, the subscription
    // lives) and a cleanup() from inside fn sees the current frameId. A
    // pending onFrame callback is a standing request for the next frame.
    frameId = nextFrameId++
    animationFrames.set(frameId, extendedFn)
    requestFrame()
    try {
      fn(tick, frame, rate)
    } catch (err) {
      console.error("Error in onFrame callback:", err)
    }
  }

  frameId = nextFrameId++
  animationFrames.set(frameId, extendedFn)
  requestFrame()

  let cleanup = () => {
    cancelled = true
    animationFrames.delete(frameId)
  }
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

// ------ Back ----------------

export type BackEvent = { preventDefault: () => void }

// App handlers for the window-level back event, as a stack: the last one
// registered is offered the event first, and the first to prevent ends the
// dispatch. Back is a pop, so the thing most recently put on screen has to
// answer for it - a dialog that opens over a screen registers after it and must
// win, and registration order tracks mount order (a parent sets up before its
// children), so reverse order also reads as innermost-first. Kept in a local
// registry rather than per-handler bus subscriptions so the default action runs
// exactly once, after the handlers have had their say.
let backHandlers: ((e: BackEvent) => void)[] = []

/**
 * Calls `fn` on the user's back intent (Android back button/gesture, the
 * desktop dev chord). Call `e.preventDefault()` when back means in-app
 * navigation right now (close a modal, previous screen); unprevented, the
 * event passes to the handler registered before this one, and if none of them
 * prevents it either, to the default action: exit(). Apps without a handler
 * exit on back everywhere, which is the correct zero-effort default.
 *
 * Handlers form a stack: the most recently registered runs first and the first
 * to prevent ends the dispatch, so each screen or overlay owns one step of the
 * back stack and none of them needs to know what the others are doing. A
 * handler that does not prevent must not act either - the event is still on its
 * way to whoever will handle it.
 *
 * Returns a cleanup function; also auto-cleans within a reactive scope.
 */
export function onBack(fn: (e: BackEvent) => void) {
  backHandlers.push(fn)
  let cleanup = () => {
    let i = backHandlers.lastIndexOf(fn)
    if (i >= 0) backHandlers.splice(i, 1)
  }
  onCleanup(cleanup)
  return cleanup
}

// ------ Window ----------------

export function attachWindow(nodeId: number) {
  // The root carries the ambient move-interest bit for global onPointerMove
  // subscribers (it is on every hit path); see core.setInterestRoot.
  setInterestRoot(nodeId)
  let unsubscribe: () => void = null!
  let unsubDown: () => void = null!
  let unsubUp: () => void = null!
  let unsubMove: () => void = null!
  let unsubEnter: () => void = null!
  let unsubLeave: () => void = null!
  let unsubWheel: () => void = null!
  let unsubKeyDown: () => void = null!
  let unsubKeyUp: () => void = null!
  let unsubBack: () => void = null!
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
    try {
      // A throwing effect must not skip renderFrame: the frame still paints
      // whatever state committed before the throw.
      flush()
    } catch (err) {
      console.error("Error in reactive flush:", err)
    }
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

    // Dispatch an event to every node on the delivered path, resolving the
    // per-node fields from the parallel wire arrays before each handler:
    // localX/localY is the pointer in that node's own frame, parentX/parentY
    // in its path-parent's frame (the frame the node's x/y live in), and
    // currentTarget the node whose handler is running. `reverse` walks the
    // root->leaf array leaf-first (bubbling), so a child handler can call
    // e.stopPropagation() to keep the event from reaching its ancestors;
    // enter/leave arrive pre-ordered and walk forward.
    interface RawPointer {
      targets: number[]
      localX: number[]
      localY: number[]
      parentX: number[]
      parentY: number[]
      [k: string]: any
    }
    let dispatchPath = (raw: RawPointer, handler: string, reverse: boolean) => {
      let { targets, localX, localY, parentX, parentY, ...e } = raw
      let stopped = false
      e.stopPropagation = () => {
        stopped = true
      }
      let n = targets.length
      for (let k = 0; k < n; k++) {
        let i = reverse ? n - 1 - k : k
        e.currentTarget = targets[i]!
        e.localX = localX[i]!
        e.localY = localY[i]!
        e.parentX = parentX[i]!
        e.parentY = parentY[i]!
        try {
          getEventHandler(targets[i]!, handler)?.(e)
        } catch (err) {
          // One throwing handler must not suppress delivery to the rest of
          // the path (or the focus/blur step that follows the loop).
          console.error(`Error in ${handler} handler:`, err)
        }
        if (stopped) break
      }
    }
    let bubble = (raw: RawPointer, handler: string) => dispatchPath(raw, handler, true)
    let dispatchOrdered = (raw: RawPointer, handler: string) => dispatchPath(raw, handler, false)

    unsubDown = on("pointerDown", (raw: RawPointer) => {
      bubble(raw, "onPointerDown")
      // Read focus AFTER per-node handlers so a tap that moves focus to a new
      // node is not immediately blurred again.
      let focused = focusedNode()
      if (focused != null && !raw.targets.includes(focused)) {
        // Outside-tap blur.
        setFocus(null)
      } else if (focused != null) {
        // A tap on the focused node is the interaction that lets a pending
        // text session raise the on-screen keyboard.
        activateTextInput()
      }
    })

    unsubUp = on("pointerUp", (raw: RawPointer) => {
      bubble(raw, "onPointerUp")
    })

    unsubMove = on("pointerMove", (raw: RawPointer) => {
      bubble(raw, "onPointerMove")
    })

    unsubEnter = on("pointerEnter", (raw: RawPointer) => {
      dispatchOrdered(raw, "onPointerEnter")
    })

    unsubLeave = on("pointerLeave", (raw: RawPointer) => {
      dispatchOrdered(raw, "onPointerLeave")
    })

    unsubWheel = on("wheel", (raw: RawPointer) => {
      bubble(raw, "onWheel")
    })

    // Key events dispatch along the focused node's ancestor chain, leaf->root
    // (the pointer bubbling contract), so a container hears keys from focused
    // descendants and the window root hears everything: <window onKeyDown> is
    // the app-global shortcut point. With nothing focused the path is the
    // window root alone - key events are never dropped. The path is resolved
    // at dispatch time from current focus (nothing to freeze: keyup follows
    // focus, as in the DOM).
    let dispatchKey = (raw: any, handler: string) => {
      let target = focusedNode() ?? nodeId
      let stopped = false
      let e = { ...raw, target, stopPropagation: () => (stopped = true) }
      let path = getNodePath(target)
      // A focused node detached this tick has no chain to the root; the
      // window root must still hear the key.
      if (path[path.length - 1] !== nodeId) path.push(nodeId)
      for (let id of path) {
        e.currentTarget = id
        getEventHandler(id, handler)?.(e)
        if (stopped) break
      }
    }

    unsubKeyDown = on("keydown", (raw: any) => dispatchKey(raw, "onKeyDown"))

    unsubKeyUp = on("keyup", (raw: any) => dispatchKey(raw, "onKeyUp"))

    unsubBack = on("back", () => {
      let prevented = false
      let e: BackEvent = {
        preventDefault: () => {
          prevented = true
        },
      }
      // Copy first: a handler may unregister (itself or others) mid-dispatch.
      // Top of the stack down, stopping as soon as one takes the event.
      let stack = [...backHandlers]
      for (let i = stack.length - 1; i >= 0 && !prevented; i--) stack[i]!(e)
      if (!prevented) exit()
    })

    unsubTextInput = on("textInput", (e: any) => {
      let id = focusedNode()
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
    setInterestRoot(null)
    if (unsubscribe) unsubscribe()
    if (unsubDown) unsubDown()
    if (unsubUp) unsubUp()
    if (unsubMove) unsubMove()
    if (unsubEnter) unsubEnter()
    if (unsubLeave) unsubLeave()
    if (unsubWheel) unsubWheel()
    if (unsubKeyDown) unsubKeyDown()
    if (unsubKeyUp) unsubKeyUp()
    if (unsubBack) unsubBack()
    if (unsubTextInput) unsubTextInput()
    if (unsubKeyboardVisibility) unsubKeyboardVisibility()
    if (unsubRefreshRate) unsubRefreshRate()
    if (unsubFirstResize) unsubFirstResize()
  })
}
