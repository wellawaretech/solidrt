import { createSignal, getOwner, onCleanup, onSettled, runWithOwner, flush } from "@solidjs/signals"
import { requestFrame, setPointerLock } from "flux:rendertree"
import { renderFrame } from "srt:render"
import { on, once } from "srt:events"
import { exit as nativeExit, background as nativeBackground } from "srt:app"
import { platform } from "flux:process"
import { getEventHandler, focusedNode, setFocus, activateTextInput, setInterestRoot } from "./core"
import { scanForOrphans, getNodePath } from "./renderer"

// ------ Lifecycle hooks ----------------

// A suspend or quit handler; the runtime waits for a returned promise.
export type LifecycleHandler = () => void | Promise<void>

// How long exit() waits for the quit handlers before leaving anyway. The
// platform-initiated ends (window close, Android onDestroy) are bounded by the
// runtime's own deadline instead; this one only keeps a hung handler from
// pinning an app that asked to leave.
const EXIT_HOOK_DEADLINE_MS = 2000

let suspendHandlers: LifecycleHandler[] = []
let quitHandlers: LifecycleHandler[] = []
// The suspend dispatch still in flight, if any: a quit that follows a
// suspend (a finishing Android activity runs onPause, then onDestroy) waits
// for it before its own handlers, so the persist that suspend started lands
// before the process ends.
let pendingSuspend: Promise<void> | null = null

function registerLifecycle(list: LifecycleHandler[], fn: LifecycleHandler) {
  list.push(fn)
  let cleanup = () => {
    let i = list.lastIndexOf(fn)
    if (i >= 0) list.splice(i, 1)
  }
  if (getOwner()) onCleanup(cleanup)
  return cleanup
}

// Runs every handler (a throw counts as a rejected promise) and settles once
// all of them have, logging the failures: one handler's error must not cut
// another's write short.
function settleHandlers(name: string, list: LifecycleHandler[]): Promise<void> {
  let results = [...list].map((fn) => {
    try {
      return Promise.resolve(fn())
    } catch (err) {
      return Promise.reject(err)
    }
  })
  return Promise.allSettled(results).then((settled) => {
    for (let r of settled) {
      if (r.status === "rejected") console.error(`Error in ${name} handler:`, r.reason)
    }
  })
}

function dispatchSuspend(): Promise<void> {
  let p = settleHandlers("onSuspend", suspendHandlers).finally(() => {
    if (pendingSuspend === p) pendingSuspend = null
  })
  pendingSuspend = p
  return p
}

function dispatchQuit(): Promise<void> {
  let inflight = pendingSuspend ?? Promise.resolve()
  return inflight.then(() => settleHandlers("onQuit", quitHandlers))
}

// The runtime asks through the event bus and waits for `done()`: the
// platform is held open (or its close budget spent) until then, so this must
// answer exactly once, whatever the handlers do. Subscribed at module load:
// core is always present, and a hook nobody listens to would leave the
// runtime waiting out its deadline on every close.
on("suspend", (e: { done: () => void }) => {
  dispatchSuspend().then(e.done, e.done)
})
on("quit", (e: { done: () => void }) => {
  dispatchQuit().then(e.done, e.done)
})

/**
 * Calls `fn` when the app is being suspended and may be killed without
 * further notice: the user switches away on Android or iOS (the platform's
 * "will enter background" moment). Never fires on desktop, where a minimized
 * app is not killed. This is the moment to persist session state - a file, a
 * database row, an HTTP request - so a later launch can pick the session up;
 * the runtime holds the platform open and waits for a returned promise, up to
 * its deadline (about ten seconds, platform-dependent), then lets the
 * platform proceed. Nothing decides for you whether to restore afterwards;
 * that is the app's call.
 *
 * Returns a cleanup function; also auto-cleans within an owned scope.
 */
export function onSuspend(fn: LifecycleHandler) {
  return registerLifecycle(suspendHandlers, fn)
}

/**
 * Calls `fn` when this app instance is ending: `exit()` (including the
 * default action of an unprevented `back` on desktop), the desktop window
 * closing, the Android activity finishing. Not when the app goes to the
 * background (`background()`, or back at the root on Android). Not a last chance to save - on mobile the
 * suspend hook has already run, and a user who quits usually does not want
 * a session kept - but the place for work that needs a real close, and the
 * runtime waits for a returned promise up to its deadline (well under a
 * second on Android, where the platform's close budget is fixed; a couple of
 * seconds on desktop). A force quit or a crash gives no signal on any
 * platform.
 *
 * Returns a cleanup function; also auto-cleans within an owned scope.
 */
export function onQuit(fn: LifecycleHandler) {
  return registerLifecycle(quitHandlers, fn)
}

/**
 * Ends the current app: back to the player in a dev client, quitting when
 * standalone or at the player itself (on Android the activity finishes, so
 * the next launch starts fresh). The default action of an unprevented `back`
 * event on desktop; call it directly to exit programmatically, e.g. after
 * intercepting back for an unsaved-changes dialog. Runs the `onQuit`
 * handlers first and leaves once their promises settle (or the deadline
 * passes); returns immediately.
 */
export function exit() {
  let deadline = new Promise<void>((resolve) => setTimeout(resolve, EXIT_HOOK_DEADLINE_MS))
  Promise.race([dispatchQuit(), deadline]).then(nativeExit, nativeExit)
}

/**
 * Leaves the current app without ending it: back to the player in a dev
 * client; otherwise the OS takes the app to the background - on Android the
 * task moves behind the others with the process kept, on desktop the window
 * minimizes. The default action of an unprevented `back` event on Android,
 * where that is what the system itself does at the root of an app. No
 * `onQuit` runs (the app is not ending); `onSuspend` fires on Android on the
 * way, as for any background.
 */
export function background() {
  nativeBackground()
}

// The default action of an unprevented back event: what the platform itself
// does at the root of an app. Android backgrounds (since Android 12);
// everywhere else back at the root ends the app.
function backDefault() {
  if (platform === "android") background()
  else exit()
}

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
 * jitter, and it is continuous across hot reloads: every tick an instance
 * sees is on one timebase, so dt is well-defined from the second call on.
 * Timers freeze together with frame callbacks under the dev tools' clock
 * control, but measure their delays against the wall clock, not this paced
 * timeline. performance.now() is not on it either: it is real elapsed time
 * for measuring work; Date.now() is calendar time.
 * Returns a cleanup function; also auto-cleans within an owned scope. Outside
 * one - an effect's apply phase, an event handler - the returned cleanup is
 * the only handle, so return it from the apply (`return onFrame(...)`) to run
 * a loop only while a condition holds.
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
  if (getOwner()) onCleanup(cleanup)
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
  if (getOwner()) onCleanup(unsubscribe)
  return unsubscribe
}

// ------ Reactive window state ----------------

// Singleton accessors over the same events as onResize / onWindowFocus. There
// is one window, so these are bare accessors rather than a createX instance.
// Lazily subscribed on first read (resize is sticky, so the first read sees the
// current value); app-lifetime, so no onCleanup. Each is created under no
// owner (runWithOwner(null)): the first read is usually a component or a
// computation, and a sticky event replays its cached value synchronously
// inside on(), so creating the signal in the reader's scope would write it
// there too - a render-time write, which the guard rejects. Detached, the
// fact belongs to the app, not to whoever read it first (the reasoning
// in full is on the ensure* functions in environment.ts).

let sizeAccessor: (() => { width: number; height: number }) | undefined
let safeAreaAccessor: (() => SafeArea) | undefined
let displayScaleAccessor: (() => number) | undefined

function ensureResizeState() {
  if (sizeAccessor) return
  runWithOwner(null, () => {
    let [size, setSize] = createSignal({ width: 0, height: 0 })
    let [safe, setSafe] = createSignal<SafeArea>({ top: 0, left: 0, right: 0, bottom: 0 })
    let [scale, setScale] = createSignal(1)
    on("resize", (e: ResizeEvent) => {
      setSize({ width: e.width, height: e.height })
      setSafe(e.safeArea)
      setScale(e.displayScale)
    })
    sizeAccessor = size
    safeAreaAccessor = safe
    displayScaleAccessor = scale
  })
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
    runWithOwner(null, () => {
      let [focused, setFocused] = createSignal(true)
      on("windowFocus", () => setFocused(true))
      on("windowBlur", () => setFocused(false))
      focusedAccessor = focused
    })
  }
  return focusedAccessor!()
}

/**
 * Lock the pointer to the window (relative mouse mode) or release it. While
 * locked the cursor is hidden and confined, clientX/clientY freeze at the
 * lock point, and mouse motion keeps reporting through the pointer events'
 * movementX/movementY - the mouse-look primitive. Window-level, no
 * permission dance: one app, one window. Observe the applied state through
 * pointerLocked(); the platform can refuse (no relative mode) and the OS
 * may drop the lock, e.g. on focus loss.
 */
export function lockPointer(locked: boolean) {
  if (typeof locked !== "boolean") throw new Error(`lockPointer: expected a boolean, got ${typeof locked}`)
  setPointerLock(locked)
}

let pointerLockedAccessor: (() => boolean) | undefined

/** Whether the pointer is currently locked (relative mouse mode), as a reactive accessor. */
export function pointerLocked(): boolean {
  if (!pointerLockedAccessor) {
    runWithOwner(null, () => {
      let [locked, setLocked] = createSignal(false)
      // Sticky event: a subscriber after the lock still observes the state.
      on("pointerLock", ({ locked }: { locked: boolean }) => setLocked(locked))
      pointerLockedAccessor = locked
    })
  }
  return pointerLockedAccessor!()
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
    runWithOwner(null, () => {
      let [height, setHeight] = createSignal(0)
      on("keyboardVisibility", ({ height: h }: { height: number }) => setHeight(h ?? 0))
      keyboardHeightAccessor = height
    })
  }
  return keyboardHeightAccessor!()
}

// Post-layout handlers run in registration order from one bus subscription,
// then pending reactive writes are flushed once. The postLayout emit is a JS
// entry inside the frame (layout has run, paint has not), and the runtime's
// microtask checkpoint only comes after the whole frame closure, so a signal
// write made by a handler would otherwise reach its node a frame late. The
// drain lives here, the way runFrame drains before renderFrame, so that no
// handler has to flush for itself. A throwing handler skips neither the
// handlers after it nor the flush.
let layoutHandlers: (() => void)[] = []
let layoutSubscribed = false

function runLayoutHandlers() {
  for (let fn of [...layoutHandlers]) {
    try {
      fn()
    } catch (err) {
      console.error("Error in onLayout handler:", err)
    }
  }
  try {
    flush()
  } catch (err) {
    console.error("Error in reactive flush:", err)
  }
}

/**
 * Fires after layout has been computed for the current frame but before paint.
 * Property writes made from this callback, direct or through signals (pending
 * reactive writes are flushed once every handler has run), are picked up by a
 * re-layout pass before painting (one extra pass; cascades beyond that paint
 * stale).
 */
export function onLayout(fn: () => void) {
  if (!layoutSubscribed) {
    layoutSubscribed = true
    on("postLayout", runLayoutHandlers)
  }
  layoutHandlers.push(fn)
  let unsubscribe = () => {
    let i = layoutHandlers.indexOf(fn)
    if (i >= 0) layoutHandlers.splice(i, 1)
  }
  if (getOwner()) onCleanup(unsubscribe)
  return unsubscribe
}

export function onWindowFocus(fn: () => void) {
  let unsubscribe = on("windowFocus", fn)
  if (getOwner()) onCleanup(unsubscribe)
  return unsubscribe
}

export function onWindowBlur(fn: () => void) {
  let unsubscribe = on("windowBlur", fn)
  if (getOwner()) onCleanup(unsubscribe)
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
 * prevents it either, to the default action: what the platform does at the
 * root of an app - `background()` on Android, `exit()` elsewhere. Apps
 * without a handler get the platform's behavior, which is the correct
 * zero-effort default.
 *
 * Handlers form a stack: the most recently registered runs first and the first
 * to prevent ends the dispatch, so each screen or overlay owns one step of the
 * back stack and none of them needs to know what the others are doing. A
 * handler that does not prevent must not act either - the event is still on its
 * way to whoever will handle it.
 *
 * Returns a cleanup function; also auto-cleans within an owned scope (outside
 * one the returned cleanup is the only handle).
 */
export function onBack(fn: (e: BackEvent) => void) {
  backHandlers.push(fn)
  let cleanup = () => {
    let i = backHandlers.lastIndexOf(fn)
    if (i >= 0) backHandlers.splice(i, 1)
  }
  if (getOwner()) onCleanup(cleanup)
  return cleanup
}

// ------ Window ----------------

// The window root's id: the key-routing fallback target and the pointer
// interest root. Set by attachWindow, moved by setWindowRoot when render()'s
// error boundary swaps the app's window for the error window and back.
let windowRootId = 0

export function setWindowRoot(nodeId: number) {
  windowRootId = nodeId
  // The root carries the ambient move-interest bit for global onPointerMove
  // subscribers (it is on every hit path); see core.setInterestRoot.
  setInterestRoot(nodeId)
}

export function attachWindow(nodeId: number) {
  setWindowRoot(nodeId)
  let unsubscribe: () => void = null!
  let unsubDown: () => void = null!
  let unsubUp: () => void = null!
  let unsubMove: () => void = null!
  let unsubEnter: () => void = null!
  let unsubLeave: () => void = null!
  let unsubWheel: () => void = null!
  let unsubTransitionEnd: () => void = null!
  let unsubKeyDown: () => void = null!
  let unsubKeyUp: () => void = null!
  let unsubBack: () => void = null!
  let unsubTextInput: () => void = null!
  let unsubKeyboardVisibility: () => void = null!
  let unsubRefreshRate: () => void = null!
  let unsubFirstResize: (() => void) | null = null

  // `bootstrap` marks the synthetic first frame (see the first-resize
  // subscription below): it flushes and paints the freshly initialized graph
  // but does not invoke onFrame callbacks, because its timestamp is not a
  // reading of the frame timeline - handing apps a zero tick gave every
  // reloaded instance one enormous dt on the next real frame
  // (okf/done/onframe-tick-reset-on-reload.md). The callbacks stay
  // registered and run on the first real render event, before the first
  // paint with their writes applied.
  function runFrame(t: number, frame: number, bootstrap = false) {
    if (!bootstrap && animationFrames.size > 0) {
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

    // A native transition finished (runtime-side animation; see the
    // `transition` prop). Target-only delivery, no bubbling: the element
    // that declared the transition is the one interested in its end.
    unsubTransitionEnd = on("transitionEnd", (raw: { target: number; property: string }) => {
      try {
        getEventHandler(raw.target, "onTransitionEnd")?.({ property: raw.property })
      } catch (err) {
        console.error("Error in onTransitionEnd handler:", err)
      }
    })

    // Key events dispatch along the focused node's ancestor chain, leaf->root
    // (the pointer bubbling contract), so a container hears keys from focused
    // descendants and the window root hears everything: <window onKeyDown> is
    // the app-global shortcut point. With nothing focused the path is the
    // window root alone - key events are never dropped. The path is resolved
    // at dispatch time from current focus (nothing to freeze: keyup follows
    // focus, as in the DOM).
    let dispatchKey = (raw: any, handler: string) => {
      let target = focusedNode() ?? windowRootId
      let stopped = false
      let e = { ...raw, target, stopPropagation: () => (stopped = true) }
      let path = getNodePath(target)
      // A focused node detached this tick has no chain to the root; the
      // window root must still hear the key.
      if (path[path.length - 1] !== windowRootId) path.push(windowRootId)
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
      if (!prevented) backDefault()
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
      queueMicrotask(() => runFrame(0, 0, true))
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
    if (unsubTransitionEnd) unsubTransitionEnd()
    if (unsubKeyDown) unsubKeyDown()
    if (unsubKeyUp) unsubKeyUp()
    if (unsubBack) unsubBack()
    if (unsubTextInput) unsubTextInput()
    if (unsubKeyboardVisibility) unsubKeyboardVisibility()
    if (unsubRefreshRate) unsubRefreshRate()
    if (unsubFirstResize) unsubFirstResize()
  })
}
