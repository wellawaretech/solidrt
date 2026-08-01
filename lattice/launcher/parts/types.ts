// Cross-screen types and small helpers shared by the launcher screens.
import { theme, policy, type StyleProps } from "@solidrt/components"

export type DevState = "idle" | "searching" | "connecting" | "connected"

// The focus-navigation ring for the launcher's custom pressables (Button
// draws its own), spread into a style. Present while focused under the
// focusRing policy; empty otherwise. Text-colored rather than primary so it
// stays visible on primary-filled surfaces.
export function focusRing(focused: boolean, radius?: number): StyleProps {
  if (!focused || !policy.focusRing) return {}
  return {
    borderWidth: 2,
    borderColor: theme.color.text,
    borderRadius: radius ?? theme.radius.md,
  }
}

// Breathing room between a scrolling list's items and the viewport's clip
// edge, in logical pixels. A focus ring is drawn on the item's own box edge,
// so an item that fills the viewport exactly leaves the ring flush against the
// clip, with nothing to spare for rounding. The other scrollers get this for
// free from their content padding; only the app list runs edge to edge.
export const LIST_GUTTER = 2

// The home screen's sub-panels: each takes over one pane rather than the whole
// screen, so the other pane keeps its content. Settings replaces the detail
// (the app list stays), connect replaces the list (a selected app's details
// stay). Both are branches of the screen state, so at most one is up.
export type HomePanel = "settings" | "connect"
// Whole screens. "home" covers the list-detail screen and its panels; "scan"
// is the only one that takes the window for itself (a full-bleed camera view).
export type Screen = "home" | "scan" | HomePanel
export type ThemeMode = "system" | "light" | "dark"

// Reading width of a content column, in logical pixels. Single-pane runs up to
// the expanded breakpoint (840), so past this width the column is centered
// rather than stretched. Shared by the screens that are one column of prose and
// controls (app list, connect), so navigating between them does not shift the
// content sideways.
export const COLUMN_MAX_WIDTH = 440

// The detail pane's column, shared by everything that fills it (app details,
// settings) so switching between them leaves the pane's content the same width.
// Wider than a prose column on purpose: it is mostly label-and-value rows, and
// those read better with the pair pushed apart than wrapped into a narrow
// column. The step in width when opening an app from the list is the cost of
// that.
export const DETAIL_MAX_WIDTH = 640

// Edge of an icon button's press box, in logical pixels: the glyphs are small,
// so these boxes are sized rather than padded. Not density-scaled - a finger is
// the same size at every density.
export const TAP_TARGET = 44

export const STATUS_TEXT: Record<DevState, string> = {
  idle: "Not connected",
  searching: "Searching...",
  connecting: "Connecting...",
  connected: "Connected",
}

// The dev server QR encodes a bare host:port; tolerate a scheme prefix and a
// trailing slash in case the encoded value ever changes.
export function normalizeAddress(raw: string): string {
  return raw
    .trim()
    .replace(/^(ws|http):\/\//, "")
    .replace(/\/+$/, "")
}
