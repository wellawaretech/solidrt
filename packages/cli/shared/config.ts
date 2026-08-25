// The launch config srt (bun) hands the dev server (flux) as its one JSON
// argument (commands/server.ts builds it, server/main.ts parses it). shared/
// holds the type-only contracts between the two programs: each has its own
// tsconfig (bun types on one side, flux types on the other), both include
// this folder, so nothing here may reference either runtime.

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
   * spawned with a JSON params argument appended (see server/rebuild.ts). */
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
