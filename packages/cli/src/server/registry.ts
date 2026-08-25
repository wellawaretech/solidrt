// The dev server registry: ~/.solidrt/servers/<key hash>/live.json, one
// folder per server key, written by the server itself once its port is
// bound and removed when it exits, so `srt client`, `srt mcp` and the
// console resolve a server without any per-project config
// (okf/backlog/cli-flux-migration.md). The process that owns the pid and the
// port owns the record, so a record whose pid is dead is stale and nothing
// else; readers still confirm with a control call. The bun readers live in
// packages/cli/src/lib/registry.ts and dev-dir.ts (the same paths and hash).
//
// The folder also remembers the last bound port so the next run tries it
// first: a project keeps its port in practice, and tunnel tickets and client
// recents stay valid, without anyone choosing a number.

import { dir, file, realpath } from "flux:fs"
import { join } from "flux:path"
import { alive, homedir, pid } from "flux:process"
import { fail } from "./args"
import type { ServerConfig } from "./config"
import type { LiveRecord } from "../types/registry"

const RECORD_FILE = "live.json"
const PORT_FILE = "port"

/** All dev-tooling state lives in ~/.solidrt (one rule on every platform). */
export function devDir(...parts: string[]): string {
  let home = homedir()
  if (!home) fail("No home directory: the dev server keeps its state under ~/.solidrt")
  return join(home, ".solidrt", ...parts)
}

/** A server's folder: `servers/<sha256 of its canonical key, truncated>/`.
 * The name is never parsed back; live.json inside is the record. */
export async function serverDirFor(key: string): Promise<string> {
  let digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(key))
  let hex = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("")
  return devDir("servers", hex.slice(0, 16))
}

/** Every readable record whose process is alive. Malformed records are
 * skipped, not fatal. */
export async function liveRecords(): Promise<LiveRecord[]> {
  let root = devDir("servers")
  let entries = await dir(root)
    .entries()
    .catch(() => [])
  let records: LiveRecord[] = []
  for (let entry of entries) {
    if (entry.type !== "directory") continue
    let record = await file(join(root, entry.name, RECORD_FILE))
      .json()
      .catch(() => null)
    if (
      typeof record?.pid === "number" &&
      typeof record?.port === "number" &&
      typeof record?.key === "string" &&
      alive(record.pid)
    ) {
      records.push(record)
    }
  }
  return records
}

/** The running server for the canonical `key`, if any. Keys from different
 * processes agree on the path, not the spelling, so compare canonical. */
export async function runningFor(key: string): Promise<LiveRecord | undefined> {
  for (let record of await liveRecords()) {
    if (record.key === key || (await realpath(record.key).catch(() => null)) === key) return record
  }
  return undefined
}

/** The port this server bound last time, or null on a first run. */
export async function rememberedPort(serverDir: string): Promise<number | null> {
  let text = await file(join(serverDir, PORT_FILE))
    .text()
    .catch(() => "")
  let port = Number(text.trim())
  return Number.isInteger(port) && port > 0 && port <= 65535 ? port : null
}

/** Write the record and remember the port. */
export async function writeRecord(config: ServerConfig, port: number, address: string) {
  await dir(config.serverDir).create()
  let record: LiveRecord = {
    pid,
    port,
    address,
    key: config.key,
    mode: config.mode,
    entry: config.entry,
    projectDir: config.projectDir,
    started: new Date().toISOString(),
  }
  await file(join(config.serverDir, RECORD_FILE)).write(JSON.stringify(record))
  await file(join(config.serverDir, PORT_FILE)).write(String(port))
}

/** Drop the record; the port memory and the tunnel key stay. */
export async function removeRecord(serverDir: string) {
  await file(join(serverDir, RECORD_FILE))
    .remove()
    .catch(() => {})
}
