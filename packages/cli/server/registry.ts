// The server's registry record: ~/.solidrt/servers/<key hash>/live.json,
// written by this process once the port is bound and removed when it exits,
// so `srt client`, `srt mcp` and the console resolve a server without any
// per-project config (okf/backlog/cli-flux-migration.md). The process that
// owns the pid and the port owns the record, so a record whose pid is dead
// is stale and nothing else; readers still confirm with a control call.
//
// The folder also remembers the last bound port so the next run tries it
// first: a project keeps its port in practice, and tunnel tickets and client
// recents stay valid, without anyone choosing a number.

import { dir, file } from "flux:fs"
import { join } from "flux:path"
import { pid } from "flux:process"
import type { ServerConfig } from "../shared/config"
import type { LiveRecord } from "../shared/registry"

const RECORD_FILE = "live.json"
const PORT_FILE = "port"

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
