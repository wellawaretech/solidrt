// Server-side state shared by the route handlers. The srt process keeps the
// bundler, watcher, and repl; this state is only what the protocol needs.

import type { ServerWebSocket } from "flux:http"

export type Config = {
  port: number
  /** Directory served by the file routes (updatable via /__internal__/reload). */
  sourceDir: string
  /** The address clients can reach this machine on (LAN IP or 127.0.0.1). */
  address: string
  proxyFiles: boolean
  proxyHttp: boolean
  /** The app entry (absolute .tsx/.jsx path) the server rebuilds on an
   * MCP-triggered reload, or undefined when srt was started without a source.
   * Moved by the repl `load` command via /__internal__/reload. */
  entry?: string
  /** Minify the rebuild output, mirroring the srt --minify flag. */
  minify: boolean
  /** How the server invokes the external bundler: [bunPath, bundleCliPath],
   * spawned with a JSON params argument appended (see rebuild.ts). */
  bundlerCmd: string[]
  /** Enable the sqlite-backed proxy cache. */
  cache: boolean
  /** Directory holding .srt-cache.db. */
  cacheDir: string
  /** Destination for captured key events, or unset when off. */
  capture?: string
  stats: boolean
  /** Accept ticket-paired clients through the p2p tunnel. */
  tunnel: boolean
}

export type ClientInfo = { platform: string; version: string; id: number; capabilities: string[] }

export let state = {
  config: undefined as unknown as Config,
  clients: new Map<ServerWebSocket, ClientInfo>(),
  nextClientId: 0,
  /**
   * The latched reload message (JSON text), replayed to late-joining clients.
   * Set by /__internal__/reload posts with `latch`, cleared by a broadcast stop.
   */
  currentReload: null as string | null,
  sourceDir: "",
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
