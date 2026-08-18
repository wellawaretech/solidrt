// Headless text-input mechanism. These primitives own the objective parts of
// an editable single-line field -- the value buffer (text + caret/selection)
// and the scroll-to-caret geometry -- and nothing with a UI opinion. Caret
// blink, keybindings, placeholder and styling are policy and belong to the
// component (the "skin") that composes these.

import { createSignal, flush, untrack } from "@solidjs/signals"
import { getBoundingBox, measureText } from "./core"
import { onLayout } from "./window"

/**
 * A text selection as anchor/focus character offsets, following the same model
 * as the platform editors (Flutter's TextSelection, the DOM Selection): the
 * anchor is where the selection started, the focus is the moving end where the
 * caret sits. A collapsed selection (anchor === focus) is a plain caret.
 */
export type Selection = { anchor: number; focus: number }

export type MoveDirection = "left" | "right" | "start" | "end"

export type TextBufferOptions = {
  /**
   * Controlled value accessor. When it returns a string, the buffer mirrors it
   * and edits flow out only through onInput (the internal text is bypassed).
   * The selection is always buffer-owned editing state regardless.
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
  /** Current selection, clamped to the text length. Collapsed = a caret. */
  selection(): Selection
  /** The focus offset (where the caret sits). */
  caret(): number
  /** Replace the current selection with text, then collapse the caret after it. */
  insertText(text: string): void
  /** Delete the selection if any, else the character before the caret. */
  deleteBackward(): void
  /** Delete the selection if any, else the character after the caret. */
  deleteForward(): void
  /** Move the caret. `extend` keeps the anchor to grow a selection (else collapses). */
  move(direction: MoveDirection, options?: { extend?: boolean }): void
  /** Set the selection directly (offsets are clamped to the text length). */
  setSelection(anchor: number, focus: number): void
  /** Replace the whole value, caret to the end. */
  setValue(next: string): void
  /** Clear to empty. */
  clear(): void
}

/**
 * An editable text buffer that bridges controlled/uncontrolled use and owns the
 * caret/selection. With a `value` accessor the buffer is controlled: edits do
 * not mutate internal text, they only call `onInput` so the owner can update its
 * source. Without one it holds the text itself. The selection is always
 * buffer-owned state and is clamped to the current text length on read, so an
 * external truncation of a controlled value cannot leave the caret dangling.
 * Every edit is clamped to `maxLength`. Mutations commit synchronously
 * (flush), so consecutive edits in one task observe each other - required for
 * burst input, where several handlers run with no microtask between them.
 */
export function createTextBuffer(options: TextBufferOptions = {}): TextBuffer {
  let initial = options.defaultValue ?? ""
  let [internalValue, setInternalValue] = createSignal(initial)
  // The caret starts at the end of the current text: for a controlled buffer
  // that is the owner's value, which defaultValue does not reflect. A
  // one-shot read by design, so untracked (the buffer is created in a
  // component body, where a bare reactive read is flagged).
  let initialCaret = untrack(() => options.value?.() ?? initial).length
  let [selectionState, setSelectionState] = createSignal<Selection>({
    anchor: initialCaret,
    focus: initialCaret,
  })

  let value = () => options.value?.() ?? internalValue()

  let selection = (): Selection => {
    let len = value().length
    let s = selectionState()
    return { anchor: Math.min(s.anchor, len), focus: Math.min(s.focus, len) }
  }

  // Ordered selection bounds [start, end).
  let range = (): [number, number] => {
    let { anchor, focus } = selection()
    return anchor <= focus ? [anchor, focus] : [focus, anchor]
  }

  let setCaret = (offset: number) => setSelectionState({ anchor: offset, focus: offset })

  // Apply a text edit and place the caret, clamping to maxLength. The flush
  // commits the writes (including a controlled owner's from onInput) before
  // returning: edits must observe each other within one task, because event
  // bursts can dispatch several handlers with no microtask between them
  // (Android IME input arrives as backspace+commit bursts; see
  // okf/backlog/event-burst-stale-signal-reads.md).
  let apply = (next: string, caret: number) => {
    let max = options.maxLength?.()
    if (max != null && next.length > max) next = next.slice(0, max)
    caret = Math.min(caret, next.length)
    if (options.value?.() == null) setInternalValue(next)
    setCaret(caret)
    options.onInput?.(next)
    flush()
  }

  return {
    value,
    selection,
    caret: () => selection().focus,

    insertText: (text) => {
      let v = value()
      let [start, end] = range()
      apply(v.slice(0, start) + text + v.slice(end), start + text.length)
    },

    deleteBackward: () => {
      let v = value()
      let [start, end] = range()
      if (start !== end) apply(v.slice(0, start) + v.slice(end), start)
      else if (start > 0) apply(v.slice(0, start - 1) + v.slice(start), start - 1)
    },

    deleteForward: () => {
      let v = value()
      let [start, end] = range()
      if (start !== end) apply(v.slice(0, start) + v.slice(end), start)
      else if (end < v.length) apply(v.slice(0, end) + v.slice(end + 1), end)
    },

    move: (direction, opts) => {
      let extend = opts?.extend ?? false
      let { anchor, focus } = selection()
      let len = value().length
      // A non-extending left/right on a range collapses to the near edge.
      if (!extend && anchor !== focus && (direction === "left" || direction === "right")) {
        setCaret(direction === "left" ? Math.min(anchor, focus) : Math.max(anchor, focus))
        flush()
        return
      }
      let next = focus
      if (direction === "left") next = Math.max(0, focus - 1)
      else if (direction === "right") next = Math.min(len, focus + 1)
      else if (direction === "start") next = 0
      else if (direction === "end") next = len
      setSelectionState({ anchor: extend ? anchor : next, focus: next })
      flush()
    },

    setSelection: (anchor, focus) => {
      let len = value().length
      setSelectionState({ anchor: Math.min(anchor, len), focus: Math.min(focus, len) })
      flush()
    },

    setValue: (next) => apply(next, next.length),
    clear: () => apply("", 0),
  }
}

export type CaretScrollInput = {
  text: string
  fontSize: number
  /** Caret offset into `text`. Defaults to the text end. */
  caret?: number
  /** Px reserved so the caret stays visible at the viewport edge. Default 0. */
  caretWidth?: number
}

/**
 * Returns the horizontal scroll offset that keeps the caret within the viewport
 * node. The offset is retained between frames and only adjusted when the caret
 * would fall outside the visible range (scrolled left when the caret runs past
 * the right edge, right when it moves before the left edge), so stationary text
 * does not jump. The viewport width and offset are computed in onLayout and the
 * synchronous flush drains the update before paint, so the scroll tracks a caret
 * or width change in the same frame. Pure geometry: no caret rendering and no
 * placeholder/visual policy.
 */
export function createCaretScroll(
  viewport: () => { id: number } | undefined,
  input: () => CaretScrollInput,
): () => number {
  let [scrollX, setScrollX] = createSignal(0)

  onLayout(() => {
    let node = viewport()
    if (!node) return
    let vw = getBoundingBox(node)?.width ?? 0
    let { text, fontSize, caret, caretWidth = 0 } = input()
    let len = text.length
    let c = caret == null ? len : Math.max(0, Math.min(caret, len))

    let totalWidth = measureText(text, { fontSize }).width
    let caretX = c >= len ? totalWidth : measureText(text.slice(0, c), { fontSize }).width
    let maxScroll = Math.max(0, totalWidth + caretWidth - vw)

    let cur = scrollX()
    let next = cur
    if (vw <= 0) {
      next = 0
    } else if (caretX < cur) {
      next = caretX
    } else if (caretX + caretWidth > cur + vw) {
      next = caretX + caretWidth - vw
    }
    next = Math.max(0, Math.min(next, maxScroll))

    if (next !== cur) setScrollX(next)
    flush()
  })

  return scrollX
}