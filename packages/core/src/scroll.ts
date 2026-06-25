// Headless scroll mechanism. This primitive owns the objective part of a
// scrollable region -- the offset and its clamping against the measured content
// and viewport sizes -- and nothing with a UI opinion. Wheel/drag input,
// momentum, scrollbars and styling are policy and belong to the component (the
// "skin") that composes this, the same way createCaretScroll backs TextInput.

import { createSignal, flush } from "@solidjs/signals"
import { getBoundingBox } from "./core"
import { onLayout } from "./window"

export type ScrollAxis = "vertical" | "horizontal" | "both"

export type ScrollOffset = { x: number; y: number }

export type ScrollOptions = {
  /** Which axes can scroll. Locked axes are pinned to 0. Default "vertical". */
  axis?: ScrollAxis
}

export type Scroll = {
  /** Current clamped offset, as a reactive accessor. */
  offset(): ScrollOffset
  /** Scroll by a delta (positive moves content up/left), clamped to range. */
  scrollBy(dx: number, dy: number): void
  /** Scroll to an absolute offset, clamped to range. */
  scrollTo(x: number, y: number): void
}

/**
 * Returns the scroll offset for a viewport node given its content node. The
 * offset is retained between frames and re-clamped in onLayout against the
 * current content-vs-viewport overflow, so the view stays valid when content
 * grows or shrinks (e.g. an offset that scrolled to the bottom snaps up when the
 * list gets shorter). scrollBy/scrollTo clamp against the most recently measured
 * range. Pure geometry: no input handling and no visual policy.
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

  // Last measured overflow, refreshed each layout. scrollBy/scrollTo clamp
  // against these between layouts; onLayout re-clamps once new sizes are known.
  let maxX = 0
  let maxY = 0

  let clamp = (x: number, y: number): ScrollOffset => ({
    x: canX ? Math.max(0, Math.min(x, maxX)) : 0,
    y: canY ? Math.max(0, Math.min(y, maxY)) : 0,
  })

  let set = (x: number, y: number) => {
    let cur = offset()
    let next = clamp(x, y)
    if (next.x !== cur.x || next.y !== cur.y) setOffset(next)
  }

  onLayout(() => {
    let vp = viewport()
    let ct = content()
    if (!vp || !ct) return
    let vb = getBoundingBox(vp)
    let cb = getBoundingBox(ct)
    if (!vb || !cb) return
    maxX = Math.max(0, cb.width - vb.width)
    maxY = Math.max(0, cb.height - vb.height)
    let cur = offset()
    let next = clamp(cur.x, cur.y)
    if (next.x !== cur.x || next.y !== cur.y) {
      setOffset(next)
      flush()
    }
  })

  return {
    offset,
    scrollBy: (dx, dy) => {
      let cur = offset()
      set(cur.x + dx, cur.y + dy)
    },
    scrollTo: (x, y) => set(x, y),
  }
}