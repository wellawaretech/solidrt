import { createSignal } from "@solidjs/signals"
import { on } from "srt:events"
import { windowSize, safeArea, displayScale, windowFocused, keyboardHeight } from "./window"

// Environment State: reactive facts about the current execution environment.
//
// Device presence comes from the runtime's sticky "inputDevices" event (init +
// hotplug). Runtimes that do not emit it leave `inputDevices` undefined, and
// the seen-flags below act as the fallback: a pointer type or key press has
// been seen this session. Seen-flags only ever go from false to true, so a
// capability derived from them can appear mid-session (e.g. the first mouse
// move) but never flickers away.
//
// `ownedWrite: true` on the signals below: each is lazily created the first
// time its ensure* function runs, which can happen inside a tracked scope
// (e.g. a memo's first read of env.inputDevices). Sticky events replay their
// cached value synchronously on subscribe (srt:events' on()), so that same
// call can immediately write the signal it just created. That's a legitimate
// internal-state write, not a stray write escaping a computation, so it opts
// out of the write-in-owned-scope guard.

/** Connected input device classes, as reported by the runtime. */
export interface InputDevices {
  keyboard: boolean
  mouse: boolean
  touch: boolean
}

export type SystemTheme = "dark" | "light" | "unknown"

export type Visibility = "visible" | "hidden"

export type Orientation = "portrait" | "portraitFlipped" | "landscape" | "landscapeFlipped" | "unknown"

let devicesAccessor: (() => InputDevices | undefined) | undefined

function ensureDevicesState() {
  if (devicesAccessor) return
  let [devices, setDevices] = createSignal<InputDevices | undefined>(undefined, { ownedWrite: true })
  // Sticky: the current state replays on subscribe, so the first read already
  // sees it on runtimes that report devices.
  on("inputDevices", (d: InputDevices) => {
    setDevices({ keyboard: !!d.keyboard, mouse: !!d.mouse, touch: !!d.touch })
  })
  devicesAccessor = devices
}

let systemThemeAccessor: (() => SystemTheme) | undefined

function ensureSystemThemeState() {
  if (systemThemeAccessor) return
  let [theme, setTheme] = createSignal<SystemTheme>("unknown", { ownedWrite: true })
  on("systemTheme", (e: { theme?: SystemTheme }) => setTheme(e.theme ?? "unknown"))
  systemThemeAccessor = theme
}

let visibilityAccessor: (() => Visibility) | undefined

function ensureVisibilityState() {
  if (visibilityAccessor) return
  let [visibility, setVisibility] = createSignal<Visibility>("visible", { ownedWrite: true })
  // Sticky. The runtime may report the same state through several platform
  // paths; the signal's equality check turns repeats into no-ops for
  // reactive consumers.
  on("visibility", (e: { state?: Visibility }) => setVisibility(e.state === "hidden" ? "hidden" : "visible"))
  visibilityAccessor = visibility
}

let orientationAccessor: (() => Orientation) | undefined

function ensureOrientationState() {
  if (orientationAccessor) return
  let [orientation, setOrientation] = createSignal<Orientation>("unknown", { ownedWrite: true })
  on("displayOrientation", (e: { orientation?: Orientation }) => {
    setOrientation(e.orientation ?? "unknown")
  })
  orientationAccessor = orientation
}

let textScaleAccessor: (() => number) | undefined

function ensureTextScaleState() {
  if (textScaleAccessor) return
  let [scale, setScale] = createSignal(1, { ownedWrite: true })
  // Sticky, like systemTheme. Guard nonsense values: a runtime bug reporting
  // 0 or a negative would otherwise collapse all text.
  on("textScale", (e: { scale?: number }) => {
    setScale(typeof e.scale === "number" && e.scale > 0 ? e.scale : 1)
  })
  textScaleAccessor = scale
}

let mouseSeenAccessor: (() => boolean) | undefined
let touchSeenAccessor: (() => boolean) | undefined

function ensurePointerState() {
  if (mouseSeenAccessor) return
  let [mouse, setMouse] = createSignal(false)
  let [touch, setTouch] = createSignal(false)
  let sawMouse = false
  let sawTouch = false
  let unsubs: (() => void)[] = []
  let note = (e: { pointerType?: string }) => {
    if (e.pointerType === "mouse" && !sawMouse) {
      sawMouse = true
      setMouse(true)
    } else if (e.pointerType === "touch" && !sawTouch) {
      sawTouch = true
      setTouch(true)
    }
    // Both types observed: nothing left to learn, stop listening.
    if (sawMouse && sawTouch) for (let u of unsubs) u()
  }
  unsubs.push(on("pointerMove", note), on("pointerDown", note))
  mouseSeenAccessor = mouse
  touchSeenAccessor = touch
}

let keyboardSeenAccessor: (() => boolean) | undefined

function ensureKeyboardState() {
  if (keyboardSeenAccessor) return
  let [keyboard, setKeyboard] = createSignal(false)
  // Soft keyboards also deliver some keydowns (Backspace, Return), so this can
  // read true on a touch-only device once the user types in a field. Only a
  // fallback: capabilities prefer runtime-reported device presence.
  let unsub = on("keydown", () => {
    setKeyboard(true)
    unsub()
  })
  keyboardSeenAccessor = keyboard
}

/**
 * Reactive Environment State: what the framework observes about where it is
 * running. Read properties inside a tracked scope (JSX, memo, effect) to re-run
 * when they change. Behavior decisions should go through `capabilities` and the
 * policy layer; read `env` directly only when the raw fact itself is needed.
 */
export let env = {
  get windowSize() {
    return windowSize()
  },
  get safeArea() {
    return safeArea()
  },
  get displayScale() {
    return displayScale()
  },
  get windowFocused() {
    return windowFocused()
  },
  get keyboardHeight() {
    return keyboardHeight()
  },
  /**
   * Connected input device classes, or undefined until the runtime reports
   * them (it does so at startup, so undefined normally means the runtime has
   * no device enumeration).
   */
  get inputDevices(): InputDevices | undefined {
    ensureDevicesState()
    return devicesAccessor!()
  },
  /**
   * The OS-level dark/light preference. Starts "unknown" and resolves once
   * the runtime's `systemTheme` event fires - read it in a tracked scope
   * (JSX, memo, effect); a top-level/untracked read will freeze at
   * "unknown" and never see the resolved value.
   */
  get systemTheme(): SystemTheme {
    ensureSystemThemeState()
    return systemThemeAccessor!()
  },
  /**
   * The OS text scaling preference (Dynamic Type / font scale), as a
   * multiplier. 1 until a runtime reports it via the sticky `textScale`
   * event; no runtime does yet.
   */
  get textScale(): number {
    ensureTextScaleState()
    return textScaleAccessor!()
  },
  /**
   * Whether the app is on screen: "hidden" while backgrounded (Android) or
   * minimized (desktop), "visible" again on return. The web's
   * `visibilityState` vocabulary without the `document` machinery - react
   * to it in a tracked scope (JSX, memo, effect).
   *
   * This is the persistence moment: there is no close event on any
   * platform (Android gives no time, desktop window close never enters
   * JS), so save state when this goes "hidden". While hidden, timers keep
   * running but no frames are produced.
   */
  get visibility(): Visibility {
    ensureVisibilityState()
    return visibilityAccessor!()
  },
  /** Orientation of the display the window is on. */
  get orientation(): Orientation {
    ensureOrientationState()
    return orientationAccessor!()
  },
  /** A mouse (or trackpad) pointer has produced events this session. */
  get mouseSeen(): boolean {
    ensurePointerState()
    return mouseSeenAccessor!()
  },
  /** A touch pointer has produced events this session. */
  get touchSeen(): boolean {
    ensurePointerState()
    return touchSeenAccessor!()
  },
  /** A key press has been delivered this session. */
  get keyboardSeen(): boolean {
    ensureKeyboardState()
    return keyboardSeenAccessor!()
  },
}
