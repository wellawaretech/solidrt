// Reading the server registry: every running dev server keeps a live.json in
// ~/.solidrt/servers/<key hash>/ (written by the server itself, see
// packages/cli/server/registry.ts). `srt client`, `srt mcp` and the launcher
// resolve a server from it by key: the canonical project root or file path
// (see mode.ts). The record is a hint; the server is authoritative, so a
// caller confirms with a control call (the x-solidrt-project header names
// the key it serves).

import { readdirSync, readFileSync, realpathSync } from "node:fs"
import { join } from "node:path"
import { serversRoot } from "./dev-dir"

export type LiveRecord = {
  pid: number
  port: number
  address: string
  key: string
  mode: "project" | "file"
  entry: string
  projectDir: string | null
  started: string
}

// Only ESRCH means the process is gone. EPERM is a live process this bridge
// may not signal (Windows reports it for other users' processes), and a
// bare try/catch would drop that healthy server from the registry.
export function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0)
    return true
  } catch (e: any) {
    return e?.code === "EPERM"
  }
}

// Two keys from different processes agree by construction on the path, not
// the spelling (drive-letter case, 8.3 names, symlinks); compare canonical.
export function sameKey(a: string, b: string): boolean {
  if (a === b) return true
  try {
    return realpathSync.native(a) === realpathSync.native(b)
  } catch {
    return false
  }
}

/** Every readable record whose process is alive. Malformed records are
 * skipped, not fatal. */
export function liveRecords(): LiveRecord[] {
  let root = serversRoot()
  let names: string[]
  try {
    names = readdirSync(root)
  } catch {
    return []
  }
  let records: LiveRecord[] = []
  for (let name of names) {
    try {
      let record = JSON.parse(readFileSync(join(root, name, "live.json"), "utf8"))
      if (
        typeof record?.pid === "number" &&
        typeof record?.port === "number" &&
        typeof record?.key === "string" &&
        pidAlive(record.pid)
      ) {
        records.push(record)
      }
    } catch {}
  }
  return records
}

export type Resolution = { ok: true; record: LiveRecord } | { ok: false; message: string }

/**
 * The server for `cwd`: the project server keyed by cwd itself; otherwise
 * the one file server whose file lies under cwd; otherwise an error that
 * lists the candidates.
 */
export function resolveFromCwd(cwd: string): Resolution {
  let records = liveRecords()
  let project = records.find((r) => r.mode === "project" && sameKey(r.key, cwd))
  if (project) return { ok: true, record: project }
  let prefix = cwd.replace(/[\\/]+$/, "") + "/"
  let files = records.filter((r) => r.mode === "file" && r.key.replace(/\\/g, "/").startsWith(prefix.replace(/\\/g, "/")))
  if (files.length === 1) return { ok: true, record: files[0]! }
  let listing =
    records.length === 0
      ? `Registry ${serversRoot()}: no running servers.`
      : `Registry ${serversRoot()}: ${records.length} running server(s).\n` +
        records.map((r) => `  port ${r.port}  ${r.mode} ${r.key}`).join("\n")
  let hint = files.length > 1 ? `${files.length} file servers run under ${cwd}; pass --port <N> to pick one.` : "Start one with srt run."
  return { ok: false, message: `No dev server for ${cwd}.\n${listing}\n${hint}` }
}
