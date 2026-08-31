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
  /** The client's storage tree on its own machine (`<data-root>/client<N>`
   * for a dev client, the install folder for the launcher or a packed app),
   * or null when it runs without writable storage or predates the field. */
  clientDir: string | null
  /** Whether its stats overlay is drawn (see POST /stats). */
  stats: boolean
  /** Its time scale as the client last reported it to POST /clock: 0 paused,
   * 1 real time. Back to 1 on every push (a reload restarts the clock). */
  timeScale: number
  /** The client's process id on its own machine. */
  pid: number | null
  /** The runtime binary it runs. */
  execPath: string | null
  /** Its machine's hostname. */
  host: string | null
  /** The OS as a person names it ("Linux (Ubuntu 24.04)", "Android 15 on Pixel 9 Pro"). */
  os: string | null
  /** The kernel version. */
  kernel: string | null
  /** The SDL video driver ("wayland", "x11", "android", "offscreen", ...). */
  videoDriver: string | null
  /** The display's nominal refresh rate in Hz as SDL reported it when the
   * client connected (what `onFrame`'s `rate` argument carries); null on a
   * runtime that predates it, or on a client that connected before its
   * window existed (a reconnect fills it in). */
  refreshRate: number | null
  /** The GPU strings as GL reports them, with the device ceilings that
   * client validates creates against (spelled like flux:gpu's `limits`
   * export); null on a client that connected before its GL context existed
   * (a reconnect fills it in). */
  gpu: GpuInfo | null
}

export type GpuInfo = { vendor: string; renderer: string; version: string; limits: GpuDeviceLimits | null }

export type GpuDeviceLimits = {
  maxTextureSize: number
  maxTextureUnits: number
  maxVertexAttribs: number
  maxAnisotropy: number
  maxVertexUniformVectors: number
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

/** POST /stats: whether the overlay is now on, and on how many clients. */
export type StatsResponse = { ok: true; active: boolean; clients: number }

/** POST /watch: whether reload-on-save is now active. */
export type WatchResponse = { ok: true; active: boolean }

/** POST /shutdown: the ack, sent just before the server exits. */
export type ShutdownResponse = { ok: true }

/** GET /snapshot and /texture: png by default, RGBA8 bytes with format=raw. */
export type ImageResponse = { width: number; height: number; pngBase64?: string; rgbaBase64?: string }

/** Every non-2xx answer. */
export type ControlError = { error: string }
