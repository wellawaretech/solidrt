// Server-side state shared by the route handlers: what the protocol needs,
// plus what main.ts resolved at startup (config.ts).

import type { ServerWebSocket } from "flux:http"
import type { ServerConfig } from "./config"
import type { ClientEntry } from "../types/control"

export let state = {
  config: undefined as unknown as ServerConfig,
  clients: new Map<ServerWebSocket, ClientEntry>(),
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
