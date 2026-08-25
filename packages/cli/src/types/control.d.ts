// The control API (/__control__/, server/control.ts) response shapes the
// MCP bridge (src/mcp/main.ts) reads fields from. Only the server-held
// answers are shaped here; queries forwarded to a client return whatever the
// client runtime answers. See bundle.d.ts for the folder.

export type ClientEntry = {
  id: number
  platform: string
  /** The client runtime's version (git describe). */
  version: string
  /** Build profile: debug or release. */
  profile: string
  capabilities: string[]
  /** Query kinds this client's runtime answers (empty on runtimes that predate
   * the advertisement); dev tools plan their verification surface from it. */
  queries: string[]
}

/** GET /clients */
export type ClientsResponse = {
  /** Identity of this server run: client ids and log cursors are only valid within one. */
  generation: number
  key: string
  mode: "project" | "file"
  entry: string
  projectDir: string | null
  /** Whether the user's own input is muted on every client (see /mute). */
  userInputMuted: boolean
  /** Whether reload-on-save is paused (see /watch). */
  watchPaused: boolean
  clients: ClientEntry[]
}

export type LogEntry = { seq: number; at: number; client: number; level: string; text: string }

/** GET /logs */
export type LogsResponse = {
  /** Consecutive identical entries come back as one, with `repeats`. */
  entries: (LogEntry & { repeats?: number })[]
  /** The next `since` cursor. */
  latest: number
  generation: number
}

/** POST /reload */
export type ReloadResponse = { ok: true; clients: number }

/** POST /load: the canonical entry now served. */
export type LoadResponse = { ok: true; entry: string; clients: number }

/** POST /mute: the mute state now in force and the clients told. */
export type MuteResponse = { ok: true; active: boolean; clients: number }

/** POST /watch: whether reload-on-save is now active. */
export type WatchResponse = { ok: true; active: boolean }

/** GET /snapshot and /texture: png by default, RGBA8 bytes with format=raw. */
export type ImageResponse = { width: number; height: number; pngBase64?: string; rgbaBase64?: string }

/** Every non-2xx answer. */
export type ControlError = { error: string }
