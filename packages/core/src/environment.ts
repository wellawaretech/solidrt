import { createSignal, runWithOwner } from "@solidjs/signals"
import { on } from "srt:events"
import { onPointerMove } from "./core"
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
// Every fact below is lazily created the first time its ensure* function
// runs, which is usually inside a component or a computation (a memo's first
// read of env.inputDevices, a HUD effect's first read of pointerLocked()).
// Created there, the fact would belong to that reader: sticky events replay
// their cached value synchronously on subscribe (srt:events' on()), so the
// on() call writes the signal it just created inside the reader's owned
// scope - a render-time write, which the guard rejects - and a root or a
// subscription created there is disposed with the reader. The facts are
// app-lifetime, so each ensure* creates under no owner (runWithOwner(null)):
// the replay lands unowned, nothing is filed under the reader, and no
// `ownedWrite` opt-out is needed.

/** Connected input device classes, as reported by the runtime. */
export interface InputDevices {
  keyboard: boolean
  mouse: boolean
  touch: boolean
  /** Whether the platform can present an on-screen keyboard. */
  screenKeyboard: boolean
}

export type SystemTheme = "dark" | "light" | "unknown"

export type Visibility = "visible" | "hidden"

export type Launch = "fresh" | "restored"

export type Orientation = "portrait" | "portraitFlipped" | "landscape" | "landscapeFlipped" | "unknown"

let devicesAccessor: (() => InputDevices | undefined) | undefined

function ensureDevicesState() {
  if (devicesAccessor) return
  runWithOwner(null, () => {
    let [devices, setDevices] = createSignal<InputDevices | undefined>(undefined)
    // Sticky: the current state replays on subscribe, so the first read already
    // sees it on runtimes that report devices.
    on("inputDevices", (d: InputDevices) => {
      setDevices({ keyboard: !!d.keyboard, mouse: !!d.mouse, touch: !!d.touch, screenKeyboard: !!d.screenKeyboard })
    })
    devicesAccessor = devices
  })
}

let systemThemeAccessor: (() => SystemTheme) | undefined

function ensureSystemThemeState() {
  if (systemThemeAccessor) return
  runWithOwner(null, () => {
    let [theme, setTheme] = createSignal<SystemTheme>("unknown")
    on("systemTheme", (e: { theme?: SystemTheme }) => setTheme(e.theme ?? "unknown"))
    systemThemeAccessor = theme
  })
}

// Fixed for the process, so a plain value rather than a signal: the sticky
// `launch` event replays synchronously on subscribe, and the runner emits it
// before any app code runs.
let launchValue: Launch | undefined

function ensureLaunchState() {
  if (launchValue) return
  runWithOwner(null, () => {
    on("launch", (e: { state?: Launch }) => {
      launchValue = e.state === "restored" ? "restored" : "fresh"
    })
  })
  // A runtime that reports no launch fact (desktop today) launches fresh.
  launchValue ??= "fresh"
}

// Fixed for the process like the launch fact (sticky `launchLink`, emitted
// before any app code runs), so a plain value rather than a signal.
let launchLinkValue: string | null | undefined

function ensureLaunchLinkState() {
  if (launchLinkValue !== undefined) return
  runWithOwner(null, () => {
    on("launchLink", (e: { link?: string | null }) => {
      launchLinkValue = typeof e.link === "string" ? e.link : null
    })
  })
  // A runtime that reports no launch link launched without one.
  launchLinkValue ??= null
}

let visibilityAccessor: (() => Visibility) | undefined

function ensureVisibilityState() {
  if (visibilityAccessor) return
  runWithOwner(null, () => {
    let [visibility, setVisibility] = createSignal<Visibility>("visible")
    // Sticky. The runtime may report the same state through several platform
    // paths; the signal's equality check turns repeats into no-ops for
    // reactive consumers.
    on("visibility", (e: { state?: Visibility }) => setVisibility(e.state === "hidden" ? "hidden" : "visible"))
    visibilityAccessor = visibility
  })
}

let orientationAccessor: (() => Orientation) | undefined

function ensureOrientationState() {
  if (orientationAccessor) return
  runWithOwner(null, () => {
    let [orientation, setOrientation] = createSignal<Orientation>("unknown")
    on("displayOrientation", (e: { orientation?: Orientation }) => {
      setOrientation(e.orientation ?? "unknown")
    })
    orientationAccessor = orientation
  })
}

let textScaleAccessor: (() => number) | undefined

function ensureTextScaleState() {
  if (textScaleAccessor) return
  runWithOwner(null, () => {
    let [scale, setScale] = createSignal(1)
    // Sticky, like systemTheme. Guard nonsense values: a runtime bug reporting
    // 0 or a negative would otherwise collapse all text.
    on("textScale", (e: { scale?: number }) => {
      setScale(typeof e.scale === "number" && e.scale > 0 ? e.scale : 1)
    })
    textScaleAccessor = scale
  })
}

let mouseSeenAccessor: (() => boolean) | undefined
let touchSeenAccessor: (() => boolean) | undefined

function ensurePointerState() {
  if (mouseSeenAccessor) return
  runWithOwner(null, () => {
    let [mouse, setMouse] = createSignal(false)
    let [touch, setTouch] = createSignal(false)
    let sawMouse = false
    let sawTouch = false
    let unsubs: (() => void)[] = []
    let unsubMove: () => void = null!
    let note = (e: { pointerType?: string }) => {
      if (e.pointerType === "mouse" && !sawMouse) {
        sawMouse = true
        setMouse(true)
        // Moves have nothing left to teach: touch is learned from downs (a
        // touch never moves without one), so drop the move subscription and
        // with it the ambient interest bit that forces move deliveries.
        unsubMove()
      } else if (e.pointerType === "touch" && !sawTouch) {
        sawTouch = true
        setTouch(true)
      }
      // Both types observed: nothing left to learn, stop listening.
      if (sawMouse && sawTouch) for (let u of unsubs) u()
    }
    // Downs always deliver, so the raw bus tap suffices; moves are gated when
    // nobody listens, so they go through onPointerMove, whose subscription
    // keeps them flowing. Unowned here, so it registers no cleanup: the
    // returned unsubscribe is the probe's only handle, as intended.
    unsubMove = onPointerMove(note)
    unsubs.push(unsubMove, on("pointerDown", note))
    mouseSeenAccessor = mouse
    touchSeenAccessor = touch
  })
}

let keyboardSeenAccessor: (() => boolean) | undefined

function ensureKeyboardState() {
  if (keyboardSeenAccessor) return
  runWithOwner(null, () => {
    let [keyboard, setKeyboard] = createSignal(false)
    // Soft keyboards also deliver some keydowns (Backspace, Return), so this can
    // read true on a touch-only device once the user types in a field. Only a
    // fallback: capabilities prefer runtime-reported device presence.
    let unsub = on("keydown", () => {
      setKeyboard(true)
      unsub()
    })
    keyboardSeenAccessor = keyboard
  })
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
   * A rendering and UI fact (nothing is on screen: stop animating, pause
   * the audio), not the persistence moment - that is `onSuspend`, which the
   * runtime waits for. While hidden no frames are produced.
   */
  get visibility(): Visibility {
    ensureVisibilityState()
    return visibilityAccessor!()
  },
  /**
   * How this process came to run. "restored": the system recreated the app
   * from a session it ended on its own (a suspended app reclaimed in the
   * background; Android's saved instance state). "fresh": launched anew, a
   * first start or one after `exit()` or a close ended the previous
   * instance, and always on desktop. Reported, not interpreted: whether to
   * load what `onSuspend` saved is the app's decision. Fixed for the
   * process, so it needs no tracked scope.
   */
  get launch(): Launch {
    ensureLaunchState()
    return launchValue!
  },
  /**
   * The link this process was started with, or null: a custom scheme link
   * the OS routed to the app (Android intent data; `srt render --link`),
   * raw, exactly as it arrived. What it names is the app's to decide, and
   * it is untrusted input: validate before acting on any part of it. A link
   * arriving while the app runs is an event instead: `onLink`. Fixed for the
   * process, so it needs no tracked scope.
   */
  get launchLink(): string | null {
    ensureLaunchLinkState()
    return launchLinkValue!
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
