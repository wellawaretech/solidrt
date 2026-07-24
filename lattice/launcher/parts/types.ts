// Cross-screen types and small helpers shared by the launcher screens.

export type DevState = "idle" | "searching" | "connecting" | "connected"
export type Screen = "home" | "scan" | "manual" | "settings"
export type ThemeMode = "system" | "light" | "dark"

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
