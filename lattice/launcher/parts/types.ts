// Cross-screen types and small helpers shared by the launcher screens.

export type DevState = "idle" | "searching" | "connecting" | "connected"
export type Screen = "home" | "scan" | "manual" | "settings"
export type ThemeMode = "system" | "light" | "dark"

// Reading width of a content column, in logical pixels. Single-pane runs up to
// the expanded breakpoint (840), so past this width the column is centered
// rather than stretched. Shared by the screens that are one column of prose and
// controls (app list, settings, connect), so navigating between them does not
// shift the content sideways.
export const COLUMN_MAX_WIDTH = 440

// The app detail view's column. Wider than a prose column on purpose: it is
// mostly label-and-value rows, and those read better with the pair pushed apart
// than wrapped into a narrow column. The step in width when opening an app from
// the list is the cost of that.
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
