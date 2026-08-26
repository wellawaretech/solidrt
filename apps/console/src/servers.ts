// The dev servers on this machine. The machine-wide registry
// (~/.solidrt/servers/<key hash>/live.json, written by each server itself,
// see packages/cli/src/server/registry.ts) is the whole answer: the record
// carries the port, the pid and what is served. A server folder outlives its
// server (it keeps the tunnel key and the remembered port), so the only thing
// left to establish is whether the process that wrote the record still runs.
import { dir, file } from "flux:fs"
import { alive, homedir } from "flux:process"
import { command } from "flux:subprocess"

/** A client connected to a dev server, as the server reports it. */
export type Client = {
  id: number
  platform: string
  /** The client runtime's version (git describe). */
  version: string
  /** Build profile: debug or release. */
  profile: string
  /** The flux modules this client's runtime was built with. */
  capabilities: string[]
  /** Control queries this client answers (empty on runtimes that predate the
   * advertisement). */
  queries: string[]
  /** Its storage tree on its own machine, or null when it did not say. */
  clientDir: string | null
  pid: number | null
  execPath: string | null
  host: string | null
  os: string | null
  kernel: string | null
  videoDriver: string | null
  gpu: { vendor: string; renderer: string; version: string } | null
}

/** A running dev server: its registry record, plus whatever its control API
 * added. `clients` is null when the port did not answer: the process is alive
 * (or the record would be gone), so that is a busy or wedged server, not an
 * absent one. */
export type Server = {
  pid: number
  port: number
  address: string
  /** The project root, or the single file, this server serves. */
  key: string
  mode: "project" | "file"
  entry: string
  projectDir: string | null
  /** ISO timestamp of the bind. */
  started: string
  clients: Client[] | null
}

/** The dev-tool state root, or null when the runtime knows no home. */
export function serversDir(): string | null {
  let home = homedir()
  return home ? `${home}/.solidrt/servers` : null
}

/** Where local dev clients keep their trees (the CLI's clientsRoot), or null
 * when the runtime knows no home. */
export function clientsDir(): string | null {
  let home = homedir()
  return home ? `${home}/.solidrt/clients` : null
}

/** How a server reads in the UI: what it serves, then its port. */
export function serverLabel(server: Server): string {
  let name = server.key.split(/[\\/]/).pop() || server.key
  return `${name} :${server.port}`
}

/** The entry, relative to what the server serves: the absolute path is the
 * same prefix for every row, so only the tail tells them apart. */
export function entryLabel(server: Server): string {
  let key = server.key.replace(/[\\/]+$/, "")
  return server.entry.startsWith(`${key}/`) ? server.entry.slice(key.length + 1) : server.entry
}

// A record is only a record: anything missing means a folder we do not
// understand, not a server.
function toServer(record: any): Server | null {
  if (typeof record?.pid !== "number" || typeof record?.port !== "number") return null
  if (typeof record?.key !== "string" || typeof record?.entry !== "string") return null
  return {
    pid: record.pid,
    port: record.port,
    address: typeof record.address === "string" ? record.address : "127.0.0.1",
    key: record.key,
    mode: record.mode === "file" ? "file" : "project",
    entry: record.entry,
    projectDir: typeof record.projectDir === "string" ? record.projectDir : null,
    started: typeof record.started === "string" ? record.started : "",
    clients: null,
  }
}

// The process that owns the port owns the record, so a dead pid is a stale
// folder and nothing else.
function dead(server: Server): boolean {
  return !alive(server.pid)
}

function text(value: unknown): string | null {
  return typeof value === "string" ? value : null
}

// One control call, the same one the MCP list_clients tool makes. It decorates
// a server the registry already established; a port that stays silent leaves
// `clients` null rather than dropping the server.
async function withClients(server: Server): Promise<Server> {
  try {
    let resp = await fetch(`http://${server.address}:${server.port}/__control__/clients`)
    if (!resp.ok) return server
    let body = await resp.json()
    if (!Array.isArray(body?.clients)) return server
    server.clients = body.clients.map((client: any) => ({
      id: client.id,
      platform: client.platform ?? "unknown",
      version: client.version ?? "unknown",
      profile: client.profile ?? "unknown",
      capabilities: Array.isArray(client.capabilities) ? client.capabilities : [],
      queries: Array.isArray(client.queries) ? client.queries : [],
      clientDir: text(client.clientDir),
      pid: typeof client.pid === "number" ? client.pid : null,
      execPath: text(client.execPath),
      host: text(client.host),
      os: text(client.os),
      kernel: text(client.kernel),
      videoDriver: text(client.videoDriver),
      gpu: client.gpu && typeof client.gpu === "object" ? client.gpu : null,
    }))
  } catch {}
  return server
}

/** Uptime, from the record's bind timestamp: "3m", "2h 14m", "4d 2h". */
export function uptime(server: Server, now: number = Date.now()): string {
  let started = Date.parse(server.started)
  if (!Number.isFinite(started)) return "?"
  let minutes = Math.max(0, Math.floor((now - started) / 60000))
  if (minutes < 60) return `${minutes}m`
  let hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ${minutes % 60}m`
  return `${Math.floor(hours / 24)}d ${hours % 24}h`
}

/** How a client reads in the UI: platform, runtime version, build profile. */
export function clientLabel(client: Client): string {
  return `#${client.id} ${client.platform} ${client.version} (${client.profile})`
}

/** The lines under a client's label: what it said about its machine, each
 * omitted when it did not say. */
export function clientFacts(client: Client): string[] {
  let lines: string[] = []
  let machine = [client.host, client.os, client.kernel].filter(Boolean).join(" ")
  if (machine) lines.push(machine)
  let gpu = client.gpu ? `${client.gpu.renderer} (${client.gpu.vendor}) ${client.gpu.version}` : null
  let graphics = [client.videoDriver, gpu].filter(Boolean).join(" | ")
  if (graphics) lines.push(graphics)
  if (client.pid !== null || client.execPath) lines.push(`pid ${client.pid ?? "?"} ${client.execPath ?? ""}`.trim())
  lines.push(client.clientDir ?? "storage not reported")
  return lines
}


/** Every registry record whose server is still running. */
export async function listServers(): Promise<Server[]> {
  let root = serversDir()
  if (!root) return []
  let entries = await dir(root)
    .entries()
    .catch(() => [])
  let servers: Server[] = []
  for (let entry of entries) {
    if (entry.type !== "directory") continue
    let record = await file(`${root}/${entry.name}/live.json`)
      .json()
      .catch(() => null)
    let server = toServer(record)
    if (server && !dead(server)) servers.push(server)
  }
  servers.sort((a, b) => a.port - b.port)
  return Promise.all(servers.map(withClients))
}

// Client numbers this console has spawned into, until those clients exit. A
// number is taken from the moment of the spawn, before the child has claimed
// its tree, so two quick presses never pick the same one. Only numbers: the
// clients are detached and outlive this console (and its reloads), so it
// never holds a handle it would lose.
let spawned = new Set<number>()

/** Whether this console can start a local client: a desktop with the dev
 * dotdir (mobile has neither that nor a toolchain). Whether the server's
 * project has a CLI to run is checked per spawn, since it is per project. */
export function canSpawnClient(): boolean {
  return clientsDir() !== null
}

// The project's installed CLI: node_modules/@solidrt/cli/bin/srt in the
// project dir or the nearest ancestor holding one, the way a module resolver
// finds it (a workspace hoists the package to the root).
async function findSrt(projectDir: string): Promise<string | null> {
  let cursor = projectDir
  while (true) {
    let srt = `${cursor}/node_modules/@solidrt/cli/bin/srt`
    if (await file(srt).exists()) return srt
    let slash = cursor.lastIndexOf("/")
    if (slash <= 0) return null
    cursor = cursor.slice(0, slash)
  }
}

// The lowest client number whose tree no live client holds. The OS lock on
// run.pid is the real claim (lattice storage.rs); the pid written inside plus
// alive() is as close as flux:fs gets, and a wrong guess costs that client a
// warning in its log, never data.
async function freeClientIndex(root: string): Promise<number> {
  for (let n = 0; ; n++) {
    if (spawned.has(n)) continue
    let text = await file(`${root}/client${n}/run.pid`)
      .text()
      .catch(() => "")
    let pid = Number(text.trim())
    if (!Number.isInteger(pid) || pid <= 0 || !alive(pid)) return n
  }
}

/** Start a local client attached to `server` through that server's own CLI
 * (`srt client`, run with bun from PATH in the served project), so the
 * console does not depend on the runner it runs in: a dev client under
 * `srt run`, the plain runner under `srt console`. The client slot is the
 * lowest free one; the data root is the CLI's default, the tree this console
 * reads. Detached: the client keeps running when this console reloads or
 * exits. Resolves with the client number once the process is launched; the
 * client itself shows in the server's list a poll later. */
export async function spawnClient(server: Server): Promise<{ client: number; pid: number | undefined }> {
  let root = clientsDir()
  if (!root) throw new Error("This machine has no dev client storage, so the console cannot start a client")
  let projectDir = server.projectDir ?? server.entry.slice(0, server.entry.lastIndexOf("/"))
  let srt = await findSrt(projectDir)
  if (!srt) throw new Error(`No @solidrt/cli installed above ${projectDir}`)
  let client = await freeClientIndex(root)
  let child = command("bun", [srt, "client", "--port", String(server.port), "--client", String(client)], {
    cwd: projectDir,
    detached: true,
  }).spawn()
  spawned.add(client)
  child.status().then(() => spawned.delete(client))
  return { client, pid: child.pid }
}
