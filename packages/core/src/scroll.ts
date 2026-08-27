// Headless scroll mechanism. This primitive owns the objective part of a
// scrollable region -- the offset and its clamping against the measured content
// and viewport sizes -- and nothing with a UI opinion. Wheel/drag input,
// momentum, scrollbars and styling are policy and belong to the component (the
// "skin") that composes this, the same way createTextEditorLayout backs TextInput.

import { createSignal } from "@solidjs/signals"
import { getBoundingBox } from "./core"
import { onLayout } from "./window"

export type ScrollAxis = "vertical" | "horizontal" | "both"

export type ScrollOffset = { x: number; y: number }

/** The web's scroll behavior word, simplified: "instant" asks the skin for no
 * motion; "auto" (the default) and "smooth" both mean its usual motion, since
 * the default already animates. */
export type ScrollBehavior = "auto" | "instant" | "smooth"

/** Target of scrollTo/scrollBy. An omitted axis keeps its offset (scrollTo)
 * or moves by 0 (scrollBy). */
export type ScrollToOptions = { x?: number; y?: number; behavior?: ScrollBehavior }

export type ScrollOptions = {
  /** Which axes can scroll. Locked axes are pinned to 0. Default "vertical". */
  axis?: ScrollAxis
}

export type Scroll = {
  /** Current clamped offset, as a reactive accessor. */
  offset(): ScrollOffset
  /** Largest reachable offset per axis (content overflow, 0 on a locked axis),
   * as a reactive accessor refreshed each layout. Watching it is how a scroll
   * policy learns that the content or the viewport changed size. */
  range(): ScrollOffset
  /** Behavior of the latest scrollTo/scrollBy, as a reactive accessor: the
   * skin reads it to withhold its motion for an instant write. Layout
   * re-clamps leave it alone. */
  behavior(): ScrollBehavior
  /** Scroll to an absolute offset, clamped to range. */
  scrollTo(options: ScrollToOptions): void
  /** Scroll by a delta (positive moves content up/left), clamped to range. */
  scrollBy(options: ScrollToOptions): void
}

/**
 * Returns the scroll offset for a viewport node given its content node. The
 * offset is retained between frames and re-clamped in onLayout against the
 * current content-vs-viewport overflow, so the view stays valid when content
 * grows or shrinks (e.g. an offset that scrolled to the bottom snaps up when the
 * list gets shorter). scrollBy/scrollTo clamp against the most recently measured
 * range. Pure geometry: no input handling and no visual policy. Anything beyond
 * clamping (following a growing log, keeping an item in view) is a policy the
 * caller writes against `range()` and `offset()`.
 *
 * The viewport node is the clipping box (overflow hidden); the content node is
 * the inner wrapper that holds the children and takes their natural size. Apply
 * the returned offset to the viewport's scrollX/scrollY.
 */
export function createScroll(
  viewport: () => { id: number } | undefined,
  content: () => { id: number } | undefined,
  options: ScrollOptions = {},
): Scroll {
  let axis = options.axis ?? "vertical"
  let canX = axis === "horizontal" || axis === "both"
  let canY = axis === "vertical" || axis === "both"

  let [offset, setOffset] = createSignal<ScrollOffset>({ x: 0, y: 0 })
  let [range, setRange] = createSignal<ScrollOffset>({ x: 0, y: 0 })
  let [behavior, setBehavior] = createSignal<ScrollBehavior>("auto")
  // Mirrors the signal: a setter's value is not readable until the flush, and
  // two writes in one batch must still compare against the latest.
  let lastBehavior: ScrollBehavior = "auto"

  // A scroll viewport with no explicit main-axis size resolves to 0 in flex
  // layout and its content silently vanishes - a classic trap (maxHeight alone
  // does not size it either). Detect it at measure time and warn once, with a
  // stack captured at creation so the warning points at the component that
  // built the scroller (the dev server remaps the frames to .tsx).
  let origin = new Error().stack ?? ""
  let warnedCollapsed = false

  // Last measured overflow, refreshed each layout. scrollBy/scrollTo clamp
  // against these between layouts; onLayout re-clamps once new sizes are known.
  // Kept as plain values beside the `range` signal because a signal write is
  // not readable until the flush, and clamping needs the number now.
  let maxX = 0
  let maxY = 0

  let clamp = (x: number, y: number): ScrollOffset => ({
    x: canX ? Math.max(0, Math.min(x, maxX)) : 0,
    y: canY ? Math.max(0, Math.min(y, maxY)) : 0,
  })

  let set = (x: number, y: number, b: ScrollBehavior = "auto") => {
    let cur = offset()
    let next = clamp(x, y)
    if (next.x !== cur.x || next.y !== cur.y) setOffset(next)
    if (b !== lastBehavior) {
      lastBehavior = b
      setBehavior(b)
    }
  }

  onLayout(() => {
    let vp = viewport()
    let ct = content()
    if (!vp || !ct) return
    let vb = getBoundingBox(vp)
    let cb = getBoundingBox(ct)
    if (!vb || !cb) return
    if (!warnedCollapsed) {
      let zeroY = canY && vb.height === 0 && cb.height > 0
      let zeroX = canX && vb.width === 0 && cb.width > 0
      if (zeroY || zeroX) {
        warnedCollapsed = true
        let axisName = zeroY ? "height" : "width"
        console.warn(
          `Scroll container resolved to ${axisName} 0, so its content is invisible. ` +
            `Give it an explicit ${axisName} or flex; maxHeight/maxWidth alone does not size it.\n${origin}`,
        )
      }
    }
    maxX = Math.max(0, cb.width - vb.width)
    maxY = Math.max(0, cb.height - vb.height)
    let r = range()
    let rx = canX ? maxX : 0
    let ry = canY ? maxY : 0
    if (r.x !== rx || r.y !== ry) setRange({ x: rx, y: ry })
    let cur = offset()
    let next = clamp(cur.x, cur.y)
    if (next.x !== cur.x || next.y !== cur.y) setOffset(next)
  })

  return {
    offset,
    range,
    behavior,
    scrollTo: (o) => {
      let cur = offset()
      set(o.x ?? cur.x, o.y ?? cur.y, o.behavior)
    },
    scrollBy: (o) => {
      let cur = offset()
      set(cur.x + (o.x ?? 0), cur.y + (o.y ?? 0), o.behavior)
    },
  }
}
