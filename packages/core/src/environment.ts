import { createSignal } from "@solidjs/signals"
import { on } from "srt:events"
import { windowSize, safeArea, displayScale, windowFocused, keyboardHeight } from "./window"

// Environment State: reactive facts about the current execution environment.
// Facts the runtime cannot yet report as device presence (no native device
// enumeration events yet) are inferred from input traffic instead: a pointer
// type or key press has been seen this session. Seen-flags only ever go from
// false to true, so a capability derived from them can appear mid-session
// (e.g. the first mouse move) but never flickers away.

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
  // read true on a touch-only device once the user types in a field. Good
  // enough until real device presence arrives from the runtime.
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
