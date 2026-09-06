// The dev server registry record: ~/.solidrt/servers/<key hash>/live.json,
// written by the server (server/registry.ts) and read by srt client, srt mcp
// and the player (src/lib/registry.ts). See bundle.d.ts for the folder.

export type LiveRecord = {
  pid: number
  port: number
  address: string
  /** The canonical project root or file path (see src/lib/mode.ts). */
  key: string
  mode: "project" | "file"
  entry: string
  projectDir: string | null
  /** ISO timestamp of the bind. */
  started: string
}
