// Headless text-input mechanism. These primitives own the objective parts of
// an editable field -- the value buffer (text + caret/selection) and the line
// and caret geometry with the scroll that keeps the caret in view -- and
// nothing with a UI opinion. Caret blink, keybindings, placeholder and styling
// are policy and belong to the component (the "skin") that composes these.

import { createMemo, createSignal, flush, untrack } from "@solidjs/signals"
import { getBoundingBox, layoutNextLine, measureText, prepareText } from "./core"
import type { MeasureTextOptions, PreparedText, TextUnit } from "flux:rendertree"
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
  /**
   * The offset one caret step left/right of `offset` in `text`: what a
   * single Left/Right, Backspace or Delete moves over. Defaults to one code
   * unit, which splits surrogate pairs and combining marks; an editor with
   * grapheme geometry (createTextEditorLayout.step) supplies the real one.
   */
  step?: (text: string, offset: number, direction: "left" | "right") => number
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

  let step = (text: string, offset: number, direction: "left" | "right"): number => {
    if (options.step) return Math.max(0, Math.min(options.step(text, offset, direction), text.length))
    return direction === "left" ? Math.max(0, offset - 1) : Math.min(text.length, offset + 1)
  }

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
      else if (start > 0) {
        let from = step(v, start, "left")
        apply(v.slice(0, from) + v.slice(start), from)
      }
    },

    deleteForward: () => {
      let v = value()
      let [start, end] = range()
      if (start !== end) apply(v.slice(0, start) + v.slice(end), start)
      else if (end < v.length) apply(v.slice(0, end) + v.slice(step(v, end, "right")), end)
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
      if (direction === "left") next = step(value(), focus, "left")
      else if (direction === "right") next = step(value(), focus, "right")
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

export type TextEditorLayoutInput = {
  text: string
  font: MeasureTextOptions
  /** Caret offset into `text`. */
  caret: number
  /** Px reserved so the caret stays visible at the viewport edge. Default 0. */
  caretWidth?: number
  /** Break lines at the viewport width; else one line per hard break. */
  wrap: boolean
}

/** One drawn line of an editor: `text.slice(start, end)` at `y`, `height` tall. */
export type EditorLine = {
  start: number
  end: number
  y: number
  height: number
  /** Ink width. */
  width: number
}

/** The caret's box in content coordinates (before scroll). */
export type CaretRect = { x: number; y: number; height: number }

export type TextEditorLayout = {
  lines(): EditorLine[]
  caret(): CaretRect
  /** Index into lines() of the line the caret sits on. */
  caretLine(): number
  /** The caret position (grapheme boundary) on line `line` nearest to content x. */
  offsetAtX(line: number, x: number): number
  /** Index of the line at content y (clamped to the first/last line). */
  lineAtY(y: number): number
  /** The caret position one grapheme left/right of `offset`; a break sequence is one step. For createTextBuffer's `step`. */
  step(offset: number, direction: "left" | "right"): number
  scrollX(): number
  scrollY(): number
}

/**
 * The line and caret geometry of an editable text, plus the scroll offsets
 * that keep the caret within the viewport node. Lines come from prepareText
 * (with caret stops) + layoutNextLine at the viewport width (or unbounded
 * when not wrapping) and are drawn by the caller, one d-text per line. A unit
 * wider than the wrap width is split into its graphemes first, so long
 * unbroken text wraps instead of overflowing. An empty text, or one ending in
 * a hard break, still gets a (blank) last line to sit the caret on. Caret
 * positions are the units' grapheme stops: the caret x, the nearest position
 * to an x, and a caret step all come from the same shaping that is drawn.
 *
 * The scroll offsets are retained between frames and only adjusted when the
 * caret would fall outside the visible range, so stationary text does not
 * jump. The viewport size is read in onLayout and the synchronous flush
 * drains the update before paint, so lines and scroll track a caret, text or
 * size change in the same frame. Pure geometry: no caret rendering and no
 * placeholder/visual policy.
 */
export function createTextEditorLayout(
  viewport: () => { id: number } | undefined,
  input: () => TextEditorLayoutInput,
): TextEditorLayout {
  let [viewportSize, setViewportSize] = createSignal({ width: 0, height: 0 }, { equals: (a, b) => a.width === b.width && a.height === b.height })
  let [scrollX, setScrollX] = createSignal(0)
  let [scrollY, setScrollY] = createSignal(0)

  let prepared = createMemo(() => {
    let { text, font } = input()
    return prepareText(text, { ...font, carets: true })
  })

  // Lines carry their unit range so the caret math walks only their units.
  type PlacedLine = EditorLine & { from: number; to: number }
  let placed = createMemo((): { units: TextUnit[]; lines: PlacedLine[] } => {
    let { text, font, wrap, caretWidth = 0 } = input()
    // Wrapped lines leave room for the caret at the end of a full line, so a
    // wrapping editor never scrolls horizontally.
    let width = wrap ? Math.max(0, viewportSize().width - caretWidth) : Infinity
    let units = wrap ? splitWide(prepared(), width) : prepared()
    let out: PlacedLine[] = []
    let y = 0
    let cursor = 0
    let line = layoutNextLine(units, cursor, width)
    let hardBreak = false
    while (line) {
      out.push({ start: line.start, end: line.end, y, height: line.height, width: line.width, from: line.from, to: line.to })
      y += line.height
      hardBreak = line.hardBreak
      line = layoutNextLine(units, line.cursor, width)
    }
    if (out.length === 0 || hardBreak) {
      let height = measureText(" ", font).height
      let n = units.units.length
      out.push({ start: text.length, end: text.length, y, height, width: 0, from: n, to: n })
    }
    return { units: units.units, lines: out }
  })
  let lines = createMemo((): EditorLine[] => placed().lines)

  // The caret stops of a line, left to right, with the pen advanced per unit;
  // duplicates at unit seams (a unit's end is the next one's start) skipped.
  let lineStops = (index: number): { offset: number; x: number }[] => {
    let { units, lines } = placed()
    let line = lines[index]
    if (!line) return []
    let stops: { offset: number; x: number }[] = []
    let pen = 0
    for (let u = line.from; u < line.to; u++) {
      let unit = units[u]!
      for (let stop of unit.carets ?? []) {
        let x = pen + stop.x
        if (stops.length && stops[stops.length - 1]!.offset === stop.offset) continue
        stops.push({ offset: stop.offset, x })
      }
      pen += unit.advance
    }
    if (stops.length === 0) stops.push({ offset: line.start, x: 0 })
    return stops
  }

  // The line an offset sits on: the first whose range extends past it, so an
  // offset on a soft-wrap boundary is the start of the next line (one offset
  // is one position; a caret does not hang after the wrap space, which would
  // take an affinity flag). The very end of the text is on the last line.
  let lineOf = (offset: number): number => {
    let ls = lines()
    for (let i = 0; i < ls.length; i++) {
      if (offset < ls[i]!.end) return i
    }
    return ls.length - 1
  }
  let caretLine = createMemo(() => lineOf(input().caret))

  // The caret sits at the last stop at or before its offset (an offset inside
  // a grapheme, e.g. from a controlled value, snaps back).
  let caret = createMemo((): CaretRect => {
    let offset = input().caret
    let index = caretLine()
    let line = lines()[index]!
    let x = 0
    for (let stop of lineStops(index)) {
      if (stop.offset > offset) break
      x = stop.x
    }
    return { x, y: line.y, height: line.height }
  })

  // Only positions that show on this line are candidates: a boundary offset
  // that displays on the next line is that line's start, not this one's end.
  let offsetAtX = (index: number, x: number): number => {
    let best = lines()[index]?.start ?? 0
    let bestDistance = Infinity
    for (let stop of lineStops(index)) {
      if (lineOf(stop.offset) !== index) continue
      let d = Math.abs(stop.x - x)
      if (d < bestDistance) {
        best = stop.offset
        bestDistance = d
      }
    }
    return best
  }

  let lineAtY = (y: number): number => {
    let ls = lines()
    let index = 0
    while (index + 1 < ls.length && ls[index + 1]!.y <= y) index++
    return index
  }

  let step = (offset: number, direction: "left" | "right"): number => {
    let { units } = placed()
    let text = input().text
    if (direction === "right") {
      for (let unit of units) {
        if (unit.end <= offset) continue
        for (let stop of unit.carets ?? []) if (stop.offset > offset) return stop.offset
        // Past the unit's shaped text: over its break characters in one step.
        return unit.end
      }
      return text.length
    }
    for (let u = units.length - 1; u >= 0; u--) {
      let unit = units[u]!
      if (unit.start >= offset) continue
      let stops = unit.carets ?? []
      for (let i = stops.length - 1; i >= 0; i--) if (stops[i]!.offset < offset) return stops[i]!.offset
      return unit.start
    }
    return 0
  }

  onLayout(() => {
    let node = viewport()
    if (!node) return
    let box = getBoundingBox(node)
    setViewportSize({ width: box?.width ?? 0, height: box?.height ?? 0 })
    flush()
    let { width: vw, height: vh } = viewportSize()
    let { caretWidth = 0, wrap } = input()
    let ls = lines()
    let contentWidth = ls.reduce((w, l) => Math.max(w, l.width), 0)
    let last = ls[ls.length - 1]!
    let contentHeight = last.y + last.height
    let c = caret()

    setScrollX(wrap ? 0 : follow(scrollX(), c.x, caretWidth, vw, contentWidth + caretWidth))
    setScrollY(follow(scrollY(), c.y, c.height, vh, contentHeight))
    flush()
  })

  return { lines, caret, caretLine, offsetAtX, lineAtY, step, scrollX, scrollY }
}

// Units wider than `width` split into one unit per grapheme (from their caret
// stops), so the greedy breaker wraps them like `<text>`'s overflowWrap
// "anywhere". Everything else is passed through as is.
function splitWide(prepared: PreparedText, width: number): PreparedText {
  if (!prepared.units.some((u) => u.width > width && (u.carets?.length ?? 0) > 2)) return prepared
  let units: TextUnit[] = []
  for (let unit of prepared.units) {
    let stops = unit.carets
    if (!(unit.width > width) || !stops || stops.length <= 2) {
      units.push(unit)
      continue
    }
    for (let i = 1; i < stops.length; i++) {
      let a = stops[i - 1]!
      let b = stops[i]!
      let last = i === stops.length - 1
      let advance = last ? unit.advance - a.x : b.x - a.x
      units.push({
        text: prepared.text.slice(a.offset, b.offset),
        start: a.offset,
        end: last ? unit.end : b.offset,
        advance,
        width: Math.max(0, Math.min(b.x, unit.width) - a.x),
        ascent: unit.ascent,
        descent: unit.descent,
        hardBreak: last && unit.hardBreak,
        glue: i === 1 && unit.glue,
        run: unit.run,
        carets: [
          { offset: a.offset, x: 0 },
          { offset: b.offset, x: b.x - a.x },
        ],
      })
    }
  }
  return { text: prepared.text, units }
}

// The scroll offset along one axis that keeps [pos, pos + size] within a
// viewport of `extent`, moved only when it is out of view and clamped to the
// content.
function follow(current: number, pos: number, size: number, extent: number, content: number): number {
  if (extent <= 0) return 0
  let next = current
  if (pos < current) next = pos
  else if (pos + size > current + extent) next = pos + size - extent
  return Math.max(0, Math.min(next, Math.max(0, content - extent)))
}
