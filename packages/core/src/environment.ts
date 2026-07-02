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

/** Connected input device classes, as reported by the runtime. */
export interface InputDevices {
  keyboard: boolean
  mouse: boolean
  touch: boolean
}

export type SystemTheme = "dark" | "light" | "unknown"

export type Orientation = "portrait" | "portraitFlipped" | "landscape" | "landscapeFlipped" | "unknown"

let devicesAccessor: (() => InputDevices | undefined) | undefined

function ensureDevicesState() {
  if (devicesAccessor) return
  let [devices, setDevices] = createSignal<InputDevices | undefined>(undefined)
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
  let [theme, setTheme] = createSignal<SystemTheme>("unknown")
  on("systemTheme", (e: { theme?: SystemTheme }) => setTheme(e.theme ?? "unknown"))
  systemThemeAccessor = theme
}

let orientationAccessor: (() => Orientation) | undefined

function ensureOrientationState() {
  if (orientationAccessor) return
  let [orientation, setOrientation] = createSignal<Orientation>("unknown")
  on("displayOrientation", (e: { orientation?: Orientation }) => {
    setOrientation(e.orientation ?? "unknown")
  })
  orientationAccessor = orientation
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
  /** The OS-level dark/light preference. */
  get systemTheme(): SystemTheme {
    ensureSystemThemeState()
    return systemThemeAccessor!()
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
