// Server-side state shared by the route handlers. The srt process keeps the
// bundler, watcher, and repl; this state is only what the protocol needs.

import type { ServerWebSocket } from "flux:http"

export type Config = {
  port: number
  /** Directory served by the file routes (updatable via /__internal__/reload). */
  sourceDir: string
  /** Project root whose assets/ folder the /assets/ route serves (updatable
   * via /__internal__/reload, moves with the repl `load`). */
  projectDir: string
  /** The address clients can reach this machine on (LAN IP or 127.0.0.1). */
  address: string
  proxyHttp: boolean
  /** The app entry (absolute .tsx/.jsx path) the server rebuilds on an
   * MCP-triggered reload, or undefined when srt was started without a source.
   * Moved by the repl `load` command via /__internal__/reload. */
  entry?: string
  /** The session's app arguments (the srt command-line tail after a bare
   * "--"), included in every reload push as flux:process argv. */
  args: string[]
  /** Minify the rebuild output, mirroring the srt --minify flag. */
  minify: boolean
  /** How the server invokes the external bundler: [bunPath, bundleCliPath],
   * spawned with a JSON params argument appended (see rebuild.ts). */
  bundlerCmd: string[]
  /** Enable the sqlite-backed proxy cache. */
  cache: boolean
  /** Directory holding the proxy cache db (the project-local .srt-data). */
  cacheDir: string
  /** Directory holding tunnel.key (the server's ~/.solidrt/servers/<port>/ folder). */
  keyDir: string
  /** Destination for captured key events, or unset when off. */
  capture?: string
  stats: boolean
  /** Accept ticket-paired clients through the p2p tunnel. */
  tunnel: boolean
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
   * Set by /__internal__/reload posts with `latch`, cleared by a broadcast stop.
   */
  currentReload: null as string | null,
  /**
   * The running bundle's sourcemap (JSON text, bundle -> .tsx sources), used
   * to remap stack traces in forwarded client logs (see control.ts). Replaced
   * on every reload; a reload without a map clears it so frames are never
   * remapped against a stale map.
   */
  currentMap: null as string | null,
  sourceDir: "",
  projectDir: "",
  serverUrl: "",
  stats: false,
  /**
   * Whether srt's file watcher may auto-reload on source changes. Agents
   * pause it (MCP watch tool -> /__control__/watch) while creating or
   * editing files; a successful /reload or /load re-enables it. srt reads
   * it via /__internal__/watch before acting on a change event.
   */
  watch: true,
  // Capture events from all connected clients share one clock (captureStartMs,
  // integer milliseconds) so they merge into one coherent timeline, tagged by
  // `device`. Streamed to disk as JSON Lines - see main.ts's "capture" handling.
  captureStartMs: 0,
  captureLastAt: 0, // ms, same clock as captureStartMs
  /** Serializes capture appends so events land on disk in arrival order. */
  captureChain: Promise.resolve(),
}
