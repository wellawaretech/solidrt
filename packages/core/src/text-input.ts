// Headless text-input mechanism. These primitives own the objective parts of
// an editable single-line field -- the value buffer and the scroll-to-caret
// geometry -- and nothing with a UI opinion. Caret blink, keybindings,
// placeholder and styling are policy and belong to the component (the "skin")
// that composes these.

import { createSignal, flush } from "@solidjs/signals"
import { getBoundingBox, measureText } from "./core"
import { onLayout } from "./window"

export type TextBufferOptions = {
  /**
   * Controlled value accessor. When it returns a string, the buffer mirrors it
   * and edits flow out only through onInput (the internal state is bypassed).
   */
  value?: () => string | undefined
  /** Initial value when uncontrolled. */
  defaultValue?: string
  /** Called with the new text after every edit, already clamped to maxLength. */
  onInput?: (value: string) => void
  /** Max length; inserts past it are clamped. */
  maxLength?: () => number | undefined
}

export type TextBuffer = {
  /** Current text: the controlled value if provided, else internal state. */
  value(): string
  /** Insert text (V1: appended at the end). */
  insertText(text: string): void
  /** Delete the last character (V1: backspace at the end). */
  deleteBackward(): void
  /** Replace the whole value. */
  setValue(next: string): void
  /** Clear to empty. */
  clear(): void
}

/**
 * An editable text buffer that bridges controlled/uncontrolled use. With a
 * `value` accessor the buffer is controlled: edits do not mutate internal state,
 * they only call `onInput` so the owner can update its source. Without one it
 * holds the text itself. Every edit is clamped to `maxLength`.
 */
export function createTextBuffer(options: TextBufferOptions = {}): TextBuffer {
  let [internalValue, setInternalValue] = createSignal(options.defaultValue ?? "")

  let value = () => options.value?.() ?? internalValue()

  let commit = (next: string) => {
    let max = options.maxLength?.()
    if (max != null && next.length > max) next = next.slice(0, max)
    if (options.value?.() == null) setInternalValue(next)
    options.onInput?.(next)
  }

  return {
    value,
    insertText: (text) => commit(value() + text),
    deleteBackward: () => {
      let v = value()
      if (v.length > 0) commit(v.slice(0, -1))
    },
    setValue: (next) => commit(next),
    clear: () => commit(""),
  }
}

export type CaretScrollInput = {
  text: string
  fontSize: number
  /** Px reserved so the caret stays visible past the text end. Default 0. */
  caretWidth?: number
}

/**
 * Returns the horizontal scroll offset that keeps the end of `text` (plus an
 * optional caret width) within the viewport node. The viewport's laid-out width
 * is read in onLayout and pushed into a signal; the synchronous flush drains the
 * resulting offset update before paint, so the scroll tracks a width change in
 * the same frame instead of trailing it. Pure geometry: no caret rendering and
 * no placeholder/visual policy.
 */
export function createCaretScroll(
  viewport: () => { id: number } | undefined,
  input: () => CaretScrollInput,
): () => number {
  let [viewportWidth, setViewportWidth] = createSignal(0)

  onLayout(() => {
    let node = viewport()
    if (!node) return
    let w = getBoundingBox(node)?.width ?? 0
    if (w !== viewportWidth()) setViewportWidth(w)
    flush()
  })

  return () => {
    let { text, fontSize, caretWidth = 0 } = input()
    let vw = viewportWidth()
    if (vw <= 0) return 0
    let tw = measureText(text, { fontSize }).width
    return Math.max(0, tw + caretWidth - vw)
  }
}