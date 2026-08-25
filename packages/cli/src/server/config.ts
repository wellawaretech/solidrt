// What the server resolved at startup from its flags (args.ts), its mode
// (mode.ts) and its environment (binaries.ts), shared with the route
// handlers through state.ts.

export type ServerConfig = {
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
  /** Where the bun-side commands run and the entry arguments they take
   * (`<entry> --project` in the project root, `<entry> --file` in the
   * entry's directory), so they resolve the same mode this server did. */
  cwd: string
  entryArgs: string[]
  /** The srt command prefix (binaries.ts) the bundle and the startup
   * typecheck run through. */
  srt: string[]
  /** An explicit --port; otherwise the remembered port is tried, then the
   * first free one. */
  port?: number
  /** Bind every interface (and announce the LAN address); default loopback. */
  lan: boolean
  /** The address clients are told to reach this server on. */
  address: string
  proxyHttp: boolean
  /** The session's app arguments (the tail after a bare "--"), included in
   * every reload push as flux:process argv. */
  args: string[]
  /** Minify the rebuild output. */
  minify: boolean
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
  /** The local client to spawn once the port is bound, or null. */
  client: { cmd: string; args: string[] } | null
}
