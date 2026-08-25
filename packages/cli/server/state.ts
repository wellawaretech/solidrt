// Server-side state shared by the route handlers: what the protocol needs,
// plus the launch config srt (bun) handed over.

import type { ServerWebSocket } from "flux:http"

export type Config = {
  /** Project mode serves the project at its root; file mode serves one file. */
  mode: "project" | "file"
  /** The canonical project root (project mode) or file path (file mode): the
   * registry key, and what every control response names. */
  key: string
  /** This server's folder under ~/.solidrt/servers/: the registry record,
   * the remembered port, the tunnel key. */
  serverDir: string
  /** The app entry (absolute path) rebuilt on a reload. */
  entry: string
  /** Directory served by the file routes (the entry's directory). */
  sourceDir: string
  /** Project root whose assets/ folder the /assets/ route serves; null in
   * file mode (no assets). */
  projectDir: string | null
  /** An explicit --port; otherwise the remembered port is tried, then 0. */
  port?: number
  /** Bind every interface (and announce the LAN address); default loopback. */
  lan: boolean
  /** This machine's LAN IPv4 (srt computes it; the server has no OS module). */
  address: string
  proxyHttp: boolean
  /** The session's app arguments (the srt command-line tail after a bare
   * "--"), included in every reload push as flux:process argv. */
  args: string[]
  /** Minify the rebuild output, mirroring the srt --minify flag. */
  minify: boolean
  /** How the server invokes the external bundler: [bunPath, bundleCliPath],
   * spawned with a JSON params argument appended (see rebuild.ts). */
  bundlerCmd: string[]
  /** The startup typecheck: [bunPath, typecheckCliPath, entry], or null when
   * the entry has no checkable program (a prebuilt bundle). */
  typecheckCmd: string[] | null
  /** Enable the sqlite-backed proxy cache. */
  cache: boolean
  /** Build outputs and the proxy cache: the project's .srt-data, or the
   * server folder in file mode. */
  cacheDir: string
  /** Destination for captured key events, or unset when off. */
  capture?: string
  stats: boolean
  /** Accept ticket-paired clients through the p2p tunnel. */
  tunnel: boolean
  /** The local client to spawn once the port is bound (`srt run`), or null
   * (`srt server`). */
  client: { cmd: string; args: string[] } | null
}

export type ClientInfo = {
  platform: string
  version: string
  profile: string
  id: number
  capabilities: string[]
  /** Query kinds this client's runtime answers (empty on runtimes that predate
   * the advertisement); dev tools plan their verification surface from it. */
  queries: string[]
}

export let state = {
  config: undefined as unknown as Config,
  clients: new Map<ServerWebSocket, ClientInfo>(),
  nextClientId: 0,
  /**
   * Identity of this server run, included in control responses that carry
   * cross-call state (client ids, log seq cursors). Both reset on restart, so
   * a consumer that sees the generation change knows its ids and cursors are
   * from a dead server and must be re-fetched.
   */
  generation: Date.now(),
  /**
   * The latched reload message (JSON text), replayed to late-joining clients.
   * Set by a successful rebuild (and the build-failure trigger).
   */
  currentReload: null as string | null,
  /**
   * The running bundle's sourcemaps (JSON text, bundle -> .tsx sources),
   * keyed by the module name stack frames cite ("main" for the app, the
   * isolate id for each isolate), used to remap stack traces in forwarded
   * client logs (see control.ts). Replaced on every reload; a reload without
   * maps clears them so frames are never remapped against a stale map.
   */
  currentMaps: null as Record<string, string> | null,
  /** The address clients reach this server on (host:port), set once bound. */
  serverUrl: "",
  stats: false,
  // Capture events from all connected clients share one clock (captureStartMs,
  // integer milliseconds) so they merge into one coherent timeline, tagged by
  // `device`. Streamed to disk as JSON Lines - see main.ts's "capture" handling.
  captureStartMs: 0,
  captureLastAt: 0, // ms, same clock as captureStartMs
  /** Serializes capture appends so events land on disk in arrival order. */
  captureChain: Promise.resolve(),
}
