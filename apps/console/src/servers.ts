// The dev servers on this machine. The machine-wide registry
// (~/.solidrt/servers/<key hash>/live.json, written by each server itself,
// see packages/cli/src/server/registry.ts) is the whole answer: the record
// carries the port, the pid and what is served. A server folder outlives its
// server (it keeps the tunnel key and the remembered port), so the only thing
// left to establish is whether the process that wrote the record still runs.
import { dir, file } from "flux:fs"
import { alive, execPath, homedir, platform } from "flux:process"
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
 * added. `clients` is null when the port did not answer: for a local server
 * the process is alive (or the record would be gone), so that is a busy or
 * wedged one, not an absent one; for a remote it is the only thing the
 * address ever tells us. */
export type Server = {
  /** Null for a remote server: a pid on another machine is not ours to ask
   * about. */
  pid: number | null
  port: number
  address: string
  /** The project root, or the single file, this server serves. Empty for a
   * remote that has not answered yet. */
  key: string
  mode: "project" | "file"
  entry: string
  projectDir: string | null
  /** ISO timestamp of the bind. Empty for a remote: the control API does not
   * report it, only the registry record does. */
  started: string
  /** Typed in as host:port rather than found in this machine's registry.
   * Read-only: everything that starts or stops a process is local. */
  remote: boolean
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

/** What identifies a server in the UI: its address and port. Not the port
 * alone - a remote may run on the same port as a local one. */
export function serverId(server: Server): string {
  return `${server.address}:${server.port}`
}

/** A typed-in remote address as host and port, or null when it is not one.
 * A port is required: dev servers have no fixed one, so a bare host is a
 * guess we refuse to make (the CLI's --server takes the same line). */
export function parseAddress(input: string): { host: string; port: number } | null {
  let value = input.trim().replace(/^https?:\/\//, "").replace(/\/+$/, "")
  let colon = value.lastIndexOf(":")
  if (colon <= 0) return null
  let host = value.slice(0, colon)
  let port = Number(value.slice(colon + 1))
  if (!Number.isInteger(port) || port <= 0 || port > 65535) return null
  return { host, port }
}

/** How a server reads in the UI: what it serves, then its port. A remote that
 * has not answered has said nothing to name it by, so it reads as its host. */
export function serverLabel(server: Server): string {
  let name = server.key.split(/[\\/]/).pop() || server.address
  return `${name} :${server.port}`
}

/** The entry, relative to what the server serves: the absolute path is the
 * same prefix for every row, so only the tail tells them apart. */
export function entryLabel(server: Server): string {
  if (!server.entry) return "Not answering"
  let key = server.key.replace(/[\\/]+$/, "")
  return server.entry.startsWith(`${key}/`) ? server.entry.slice(key.length + 1) : server.entry
}

/** Where a server runs, for its detail line. The same line for every server:
 * a remote reports no pid and no bind time, so those read as "?" rather than
 * turning the pane into a different pane. */
export function serverWhere(server: Server): string {
  return `pid ${server.pid ?? "?"} on ${server.address}:${server.port}, up ${uptime(server)}`
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
    remote: false,
    clients: null,
  }
}

// A typed-in address, before anything has answered on it. Everything a
// registry record would have told us is left empty until the control call
// fills it in.
function remoteServer(host: string, port: number): Server {
  return {
    pid: null,
    port,
    address: host,
    key: "",
    mode: "project",
    entry: "",
    projectDir: null,
    started: "",
    remote: true,
    clients: null,
  }
}

// The process that owns the port owns the record, so a dead pid is a stale
// folder and nothing else. Registry records only: a remote has no pid, and a
// pid on another machine would not be this one's to look up anyway.
function dead(server: Server): boolean {
  return server.pid === null || !alive(server.pid)
}

function text(value: unknown): string | null {
  return typeof value === "string" ? value : null
}

// One control call, the same one the MCP list_clients tool makes. It decorates
// a server the registry already established; a port that stays silent leaves
// `clients` null rather than dropping the server. For a remote the same call
// is also the introduction: there is no record on this machine, so what it
// serves comes from the answer or not at all.
async function withClients(server: Server): Promise<Server> {
  try {
    let resp = await fetch(`http://${server.address}:${server.port}/__control__/clients`)
    if (!resp.ok) return server
    let body = await resp.json()
    if (!Array.isArray(body?.clients)) return server
    if (server.remote) {
      server.key = text(body.key) ?? ""
      server.mode = body.mode === "file" ? "file" : "project"
      server.entry = text(body.entry) ?? ""
      server.projectDir = text(body.projectDir)
    }
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


/** Ask a typed-in address what it serves. A server comes back either way:
 * `clients` null means nothing answered there, which is all the caller needs
 * to decide whether it is worth keeping. */
export async function probeRemote(host: string, port: number): Promise<Server> {
  return withClients(remoteServer(host, port))
}

/** Every registry record whose server is still running, followed by the
 * remote addresses the caller is holding. A remote that duplicates a local
 * record is dropped: the registry already has the better answer, and two rows
 * for one server is a lie. */
export async function listServers(remotes: string[] = []): Promise<Server[]> {
  let root = serversDir()
  let entries = root
    ? await dir(root)
        .entries()
        .catch(() => [])
    : []
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
  let local = await Promise.all(servers.map(withClients))
  let taken = new Set(local.map(serverId))
  let remote = await Promise.all(
    remotes.flatMap((address) => {
      let parsed = parseAddress(address)
      if (!parsed || taken.has(`${parsed.host}:${parsed.port}`)) return []
      return [probeRemote(parsed.host, parsed.port)]
    }),
  )
  return [...local, ...remote]
}

// Client numbers this console has spawned into, until those clients exit. A
// number is taken from the moment of the spawn, before the child has claimed
// its tree, so two quick presses never pick the same one. Only numbers: the
// clients are detached and outlive this console (and its reloads), so it
// never holds a handle it would lose.
let spawned = new Set<number>()

/** Whether this console can start a client here: a machine with the dev
 * dotdir to keep its tree in, and a runtime that can name its own
 * executable, next to which the client binary is looked for. Whether it is
 * actually there is checked per spawn. */
export function canSpawnClient(): boolean {
  return clientsDir() !== null && execPath !== null
}

// The dev client binary: the solidrt-go next to the executable this console
// runs in. Under `srt run` that executable IS solidrt-go; under `srt console`
// it is the plain solidrt runner, whose build discards --dev-server, and
// solidrt-go ships beside it in the checkout and the platform package alike.
// A console packed with its runner embedded has no sibling, so null then.
async function clientBinary(): Promise<string | null> {
  if (!execPath) return null
  let dir = execPath.slice(0, Math.max(execPath.lastIndexOf("/"), execPath.lastIndexOf("\\")))
  let bin = `${dir}/solidrt-go${platform === "win32" ? ".exe" : ""}`
  return (await file(bin).exists()) ? bin : null
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

/** Start a client on THIS machine attached to `server`, wherever the server
 * runs: the dev client binary beside this console's own (what `srt client`
 * would have resolved) pointed at the server's address. The served project
 * has nothing to do with it, and for a remote it is not on this machine
 * anyway. The client slot is the lowest free one and the data root is the
 * tree this console reads, the same two the CLI would have passed. Detached:
 * the client keeps running when this console reloads or exits. Resolves with
 * the client number once the process is launched; the client itself shows in
 * the server's list a poll later. */
export async function spawnClient(server: Server): Promise<{ client: number; pid: number | undefined }> {
  let root = clientsDir()
  if (!root) throw new Error("This machine has no dev client storage, so the console cannot start a client")
  let bin = await clientBinary()
  if (!bin) throw new Error(`No solidrt-go next to ${execPath ?? "this runtime"}, so there is no client to start`)
  let client = await freeClientIndex(root)
  let child = command(bin, [
    "--data-root",
    root,
    "--client",
    String(client),
    "--dev-server",
    `${server.address}:${server.port}`,
  ], { detached: true }).spawn()
  spawned.add(client)
  child.status().then(() => spawned.delete(client))
  return { client, pid: child.pid }
}
