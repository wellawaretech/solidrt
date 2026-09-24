// Headless text-input mechanism. These primitives own the objective parts of
// an editable field -- the value buffer (text + caret/selection) and the line
// and caret geometry with the scroll that keeps the caret in view -- and
// nothing with a UI opinion. Caret blink, keybindings, placeholder and styling
// are policy and belong to the component (the "skin") that composes these.

import { createMemo, createSignal, flush, untrack } from "@solidjs/signals"
import { getLayoutBox, layoutNextLine, measureText, prepareText, unitInk } from "./core"
import type { MeasureTextOptions, PreparedText, TextRunRange, TextUnit } from "flux:rendertree"
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
  /**
   * Called before every edit with the range it replaces and the text going
   * in (already clamped to maxLength), for owners that keep parallel state
   * over the text (a rich text document's attributed runs). setValue/clear
   * report a whole-text replace.
   */
  onReplace?: (start: number, end: number, text: string) => void
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

  // Every edit is a replace of [start, end) by `text`, caret after it. The
  // inserted text is clamped to what maxLength leaves room for. The flush
  // commits the writes (including a controlled owner's from onInput) before
  // returning: edits must observe each other within one task, because event
  // bursts can dispatch several handlers with no microtask between them
  // (Android IME input arrives as backspace+commit bursts; see
  // okf/backlog/event-burst-stale-signal-reads.md).
  let replace = (start: number, end: number, text: string) => {
    let v = value()
    let max = options.maxLength?.()
    if (max != null) text = text.slice(0, Math.max(0, max - (v.length - (end - start))))
    options.onReplace?.(start, end, text)
    let next = v.slice(0, start) + text + v.slice(end)
    if (options.value?.() == null) setInternalValue(next)
    setCaret(start + text.length)
    options.onInput?.(next)
    flush()
  }

  return {
    value,
    selection,
    caret: () => selection().focus,

    insertText: (text) => {
      let [start, end] = range()
      replace(start, end, text)
    },

    deleteBackward: () => {
      let [start, end] = range()
      if (start !== end) replace(start, end, "")
      else if (start > 0) replace(step(value(), start, "left"), start, "")
    },

    deleteForward: () => {
      let v = value()
      let [start, end] = range()
      if (start !== end) replace(start, end, "")
      else if (end < v.length) replace(end, step(v, end, "right"), "")
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

    setValue: (next) => replace(0, value().length, next),
    clear: () => replace(0, value().length, ""),
  }
}

export type TextEditorLayoutInput = {
  text: string
  font: MeasureTextOptions
  /** Styled ranges over `text` (prepareText `runs`): the geometry then follows per-run fonts. */
  runs?: TextRunRange[]
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

/** One line's slice of a selection highlight, in content coordinates (before scroll). */
export type SelectionRect = { x: number; y: number; width: number; height: number }

export type TextEditorLayout = {
  lines(): EditorLine[]
  caret(): CaretRect
  /** Index into lines() of the line the caret sits on. */
  caretLine(): number
  /** The caret position (grapheme boundary) on line `line` nearest to content x. */
  offsetAtX(line: number, x: number): number
  /** Highlight boxes for the anchor..focus range, one per touched line; empty when collapsed. */
  selectionRects(anchor: number, focus: number): SelectionRect[]
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
 * jump. They derive from the caret, the lines and the viewport size, so a
 * frame that changes none of them does no scroll work. The viewport size is
 * the node's layout box (the untransformed solved box: the lines are drawn in
 * the node's own frame, so an ancestor's scale must not change the wrap
 * width), read in onLayout; the post-layout flush drains the update before
 * paint, so lines and scroll track a caret, text or size change in the same
 * frame. A size change that re-breaks nothing (an empty field, lines that
 * still fit, a height-only change) stops at the line placement: the lines,
 * the caret and everything the caller derives from them keep their values
 * and re-run nothing, and the placement reads the break inputs through a
 * memo of its own rather than `input()` (which a caller builds per read),
 * so a field inside a resizing box costs one placement of its units per
 * frame, not a flush of its whole graph. Pure geometry: no caret rendering
 * and no placeholder/visual policy.
 */
export function createTextEditorLayout(
  viewport: () => { id: number } | undefined,
  input: () => TextEditorLayoutInput,
): TextEditorLayout {
  // Width and height apart: wrapping depends on the width alone, so a
  // height-only change (a field growing with its content) re-breaks nothing.
  let [viewportWidth, setViewportWidth] = createSignal(0)
  let [viewportHeight, setViewportHeight] = createSignal(0)

  // The font as a value: input() builds a fresh options object per read, so
  // field equality is what lets the metrics below key on the font itself.
  let font = createMemo(() => input().font, { equals: sameOptions })

  // One space in the font: its height sits the synthesized blank line, its
  // width is the selection's break sliver. Measured once per font, not once
  // per placement.
  let space = createMemo(() => measureText(" ", font()))

  let prepared = createMemo(() => {
    let { text, runs } = input()
    return prepareText(text, { ...font(), runs, carets: true })
  })

  // What the breaks depend on, apart from the rest of the input: the caller
  // builds input() per read (a field's value, font, caret and runs - some
  // twenty reactive reads), and a placement re-run for a viewport width
  // change must not pay for that. Equal by field, so only a real change
  // reaches the placement.
  let breaking = createMemo(
    (): BreakInput => {
      let { text, wrap, caretWidth = 0 } = input()
      return { text, wrap, caretWidth }
    },
    { equals: sameBreakInput },
  )

  // Lines carry their unit range so the caret math walks only their units.
  // Equal placements (same units, same breaks and metrics) keep the previous
  // value, so nothing downstream re-runs for a width change that changed
  // no line. Within a changed placement, a line equal to its predecessor
  // keeps the previous object: a row bound to it (a non-keyed <For> item)
  // sees no change and writes nothing, so a moved break costs the lines it
  // touched, not every line of the field.
  type PlacedLine = EditorLine & { from: number; to: number; hardBreak: boolean }
  // `holds` is the range of wrap widths [min, max) the breaker yields these
  // same lines at (see holdRange); the layout handler skips a width inside it.
  type Placement = { units: TextUnit[]; lines: PlacedLine[]; holds: [number, number] }
  let placed = createMemo(
    (prev): Placement => {
      let { text, wrap, caretWidth } = breaking()
      // Wrapped lines leave room for the caret at the end of a full line, so a
      // wrapping editor never scrolls horizontally.
      let width = wrap ? Math.max(0, viewportWidth() - caretWidth) : Infinity
      let units = wrap ? splitWide(prepared(), width) : prepared()
      let out: PlacedLine[] = []
      let y = 0
      let cursor = 0
      let line = layoutNextLine(units, cursor, width)
      let hardBreak = false
      while (line) {
        out.push({
          start: line.start,
          end: line.end,
          y,
          height: line.height,
          width: line.width,
          from: line.from,
          to: line.to,
          hardBreak: line.hardBreak,
        })
        y += line.height
        hardBreak = line.hardBreak
        line = layoutNextLine(units, line.cursor, width)
      }
      if (out.length === 0 || hardBreak) {
        let n = units.units.length
        out.push({ start: text.length, end: text.length, y, height: space().height, width: 0, from: n, to: n, hardBreak: false })
      }
      if (prev && prev.units === units.units) {
        for (let i = 0; i < out.length && i < prev.lines.length; i++) {
          if (sameLine(prev.lines[i]!, out[i]!)) out[i] = prev.lines[i]!
        }
      }
      // Split units are the width's own (a unit wider than it), so those
      // lines hold for no other width.
      let holds: [number, number] = units.units === prepared().units ? holdRange(units.units, out) : [Infinity, Infinity]
      return { units: units.units, lines: out, holds }
    },
    { equals: samePlacement },
  )
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

  // The x of the last stop at or before `offset` on line `index` (an offset
  // inside a grapheme, e.g. from a controlled value, snaps back).
  let xAt = (index: number, offset: number): number => {
    let x = 0
    for (let stop of lineStops(index)) {
      if (stop.offset > offset) break
      x = stop.x
    }
    return x
  }

  let caret = createMemo((): CaretRect => {
    let offset = input().caret
    let index = caretLine()
    let line = lines()[index]!
    return { x: xAt(index, offset), y: line.y, height: line.height }
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

  // One box per line the range touches, offsets snapped to stops like the
  // caret. A line the selection continues past (every one but the last)
  // highlights to its ink end plus a space width, so the selected break - a
  // newline, or the wrap the range crosses - is visible, and a selected empty
  // line shows as that sliver alone. The last line ends at the focus stop; a
  // range ending on a wrap boundary owns a zero-width box there (the boundary
  // offset displays on the next line) and drops it.
  let selectionRects = (anchor: number, focus: number): SelectionRect[] => {
    let start = Math.min(anchor, focus)
    let end = Math.max(anchor, focus)
    if (start >= end) return []
    let ls = lines()
    let first = lineOf(start)
    let last = lineOf(end)
    let breakWidth = space().width
    let out: SelectionRect[] = []
    for (let i = first; i <= last; i++) {
      let line = ls[i]!
      let x = i === first ? xAt(i, start) : 0
      let width = i === last ? xAt(i, end) - x : Math.max(line.width - x, 0) + breakWidth
      if (width <= 0) continue
      out.push({ x, y: line.y, width, height: line.height })
    }
    return out
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

  // Each offset follows from its own previous value: `follow` moves it only
  // when the caret has left the visible range, so the memo carries the
  // retained position across recomputes.
  let scrollX = createMemo((prev: number | undefined): number => {
    let { caretWidth, wrap } = breaking()
    if (wrap) return 0
    let contentWidth = lines().reduce((w, l) => Math.max(w, l.width), 0)
    let c = caret()
    return follow(prev ?? 0, c.x, caretWidth, viewportWidth(), contentWidth + caretWidth)
  })
  let scrollY = createMemo((prev: number | undefined): number => {
    let ls = lines()
    let last = ls[ls.length - 1]!
    let c = caret()
    return follow(prev ?? 0, c.y, c.height, viewportHeight(), last.y + last.height)
  })

  // Whether the lines placed at the last written width are what the breaker
  // yields at `width` too. Such a width is not written: the write would mark
  // the whole graph below the placement for a re-check that finds nothing,
  // which is what a field inside a resizing box paid per frame. So
  // viewportWidth lags the box by less than a break while wrapping, and a
  // placement re-run for a text change at the lagging width is checked here
  // again on that frame and corrected before paint (writes from onLayout
  // land in the same frame's re-layout; the same holds for the height
  // below). Unwrapped, the width is the horizontal scroll's extent and is
  // always written.
  let keepsBreaks = (width: number): boolean => {
    let { wrap, caretWidth } = breaking()
    if (!wrap) return false
    let [min, max] = placed().holds
    let w = Math.max(0, width - caretWidth)
    return w >= min && w < max
  }

  // The same for the height: a height at which the retained scroll offset
  // still keeps the caret in view is not written, so the scroll graph stays
  // untouched while a short field's box grows and shrinks.
  let keepsScroll = (height: number): boolean => {
    let ls = lines()
    let last = ls[ls.length - 1]!
    let c = caret()
    let current = scrollY()
    return follow(current, c.y, c.height, height, last.y + last.height) === current
  }

  onLayout(() => {
    let node = viewport()
    if (!node) return
    let box = getLayoutBox(node)
    let width = box?.width ?? 0
    let height = box?.height ?? 0
    if (!keepsBreaks(width)) setViewportWidth(width)
    if (!keepsScroll(height)) setViewportHeight(height)
  })

  return { lines, caret, caretLine, offsetAtX, selectionRects, lineAtY, step, scrollX, scrollY }
}

// Wrap units wider than `width` (through their glued pieces) split into one
// unit per grapheme (from their caret stops), so the greedy breaker wraps
// them like `<text>`'s overflowWrap "anywhere". Everything else is passed
// through as is.
function splitWide(prepared: PreparedText, width: number): PreparedText {
  let all = prepared.units
  if (!all.some((u, i) => !u.glue && unitInk(all, i) > width)) return prepared
  let wide = false
  let units: TextUnit[] = []
  for (let u = 0; u < all.length; u++) {
    let unit = all[u]!
    if (!unit.glue) wide = unitInk(all, u) > width
    let stops = unit.carets
    if (!wide || !stops || stops.length <= 2) {
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

// The wrap widths [min, max) at which the greedy breaker yields exactly
// `lines` over `units`: every line's ink fits (min), and no line's first
// unit fits at the end of the line before it (max), except across a hard
// break, where it never joins. Greedy breaking is decided line by line on
// just those two tests, so inside the range the placement is unchanged.
function holdRange(units: TextUnit[], lines: { from: number; to: number; width: number; hardBreak: boolean }[]): [number, number] {
  let min = 0
  let max = Infinity
  for (let i = 0; i < lines.length; i++) {
    let line = lines[i]!
    if (line.width > min) min = line.width
    let next = lines[i + 1]
    if (!next || line.hardBreak || next.from >= units.length) continue
    let pen = 0
    for (let u = line.from; u < line.to; u++) pen += units[u]!.advance
    let join = pen + unitInk(units, next.from)
    if (join < max) max = join
  }
  return [min, max]
}

type BreakInput = { text: string; wrap: boolean; caretWidth: number }

function sameBreakInput(a: BreakInput, b: BreakInput): boolean {
  return a.text === b.text && a.wrap === b.wrap && a.caretWidth === b.caretWidth
}

// Font options compared field by field: the same keys with the same values.
function sameOptions(a: MeasureTextOptions, b: MeasureTextOptions): boolean {
  let ka = Object.keys(a) as (keyof MeasureTextOptions)[]
  let kb = Object.keys(b) as (keyof MeasureTextOptions)[]
  return ka.length === kb.length && ka.every((k) => a[k] === b[k])
}

// The same break (text range) at the same place with the same metrics.
function sameLine(a: EditorLine, b: EditorLine): boolean {
  return a.start === b.start && a.end === b.end && a.y === b.y && a.height === b.height && a.width === b.width
}

// Two placements of the same units with the same lines, break for break and
// metric for metric.
function samePlacement(a: { units: TextUnit[]; lines: EditorLine[] }, b: { units: TextUnit[]; lines: EditorLine[] }): boolean {
  if (a.units !== b.units || a.lines.length !== b.lines.length) return false
  return a.lines.every((l, i) => sameLine(l, b.lines[i]!))
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
