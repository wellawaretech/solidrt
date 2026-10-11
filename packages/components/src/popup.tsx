import { env } from "@solidrt/core"
import type { BoundingBox, Scroll } from "@solidrt/core"
import { theme } from "./theme"
import { ScrollView } from "./scroll-view"

// The anchored popups (Tooltip, Select's dropdown, ContextMenu) share one
// placement: portal at the window root, parked offscreen until measured in
// onLayout, then moved with the x/y paint transforms so tracking the anchor
// never reflows. The math lives here so every popup keeps the same rules:
// a side is taken only when it has the room, both axes are clamped to the
// window margin, and a popup taller than any side caps itself to the room
// the placement reports.

// Distance between an anchor and the popup it opens.
export let popupGap = () => theme.spacing.sm
// Minimum distance kept between a popup and the window edges.
export let popupMargin = () => theme.spacing.sm

export type PopupSide = "top" | "bottom"

export interface Placement {
  x: number
  y: number
  // The side of the anchor the popup sits on.
  side: PopupSide
  // Height available on that side, between the anchor (plus gap) and the
  // window margin. A popup taller than this caps its content to it; `y`
  // already assumes it does.
  room: number
}

export interface PlaceOptions {
  // Preferred side (default "bottom"). Kept when the popup fits there;
  // otherwise the other side when it fits, else whichever has more room.
  side?: PopupSide
  // Horizontal alignment to the anchor: on its left edge (default) or
  // centered on it. Clamped to the window either way.
  align?: "start" | "center"
  // Distance from the anchor's edge; default popupGap(). A popup anchored
  // at a point (a context menu at the pointer) passes 0.
  gap?: number
}

/**
 * Where a popup of `size` goes next to `anchor`, both in window
 * coordinates (getBoundingBox from the window root, inside onLayout).
 */
export function placePopup(anchor: BoundingBox, size: { width: number; height: number }, options: PlaceOptions = {}): Placement {
  let gap = options.gap ?? popupGap()
  let margin = popupMargin()
  let window = env.windowSize
  let roomBelow = window.height - margin - (anchor.y + anchor.height + gap)
  let roomAbove = anchor.y - gap - margin
  let preferred = options.side ?? "bottom"
  let fits = (side: PopupSide) => size.height <= (side === "bottom" ? roomBelow : roomAbove)
  let other: PopupSide = preferred === "bottom" ? "top" : "bottom"
  let side: PopupSide = fits(preferred) ? preferred : fits(other) ? other : roomBelow >= roomAbove ? "bottom" : "top"
  let room = side === "bottom" ? roomBelow : roomAbove
  let x = options.align === "center" ? anchor.x + anchor.width / 2 - size.width / 2 : anchor.x
  x = Math.round(Math.min(Math.max(x, margin), window.width - size.width - margin))
  let y = Math.round(side === "bottom" ? anchor.y + anchor.height + gap : anchor.y - gap - Math.min(size.height, room))
  return { x, y, side, room }
}

export interface PopupListProps {
  // The list's height: its rows' natural height capped to the room, as the
  // owner measured and resolved it in its onLayout; null before the first
  // layout.
  height: number | null
  // The rows' column, for the owner to measure (its box is the natural
  // height, whatever the viewport shows).
  listRef?: (node: { id: number }) => void
  scrollRef?: (scroll: Scroll) => void
  children?: any
}

/**
 * The rows of a popup: a column that scrolls once taller than the room it
 * has. A scroll viewport has no natural height (its minimum is 0, so a max
 * alone collapses it: see createScroll), so the owner measures the column
 * through `listRef` and sizes the list explicitly. Until measured it stands
 * at the window height, so the viewport never reads 0 while its popup is
 * still parked offscreen or hidden.
 */
export function PopupList(props: PopupListProps) {
  let height = () => (props.height == null ? env.windowSize.height : Math.max(0, props.height))
  return (
    <ScrollView layout={{ height: height() }} scrollRef={props.scrollRef}>
      <view ref={props.listRef} flexDirection="column">
        {props.children}
      </view>
    </ScrollView>
  )
}
