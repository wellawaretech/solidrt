import { file } from "flux:fs"
import { state } from "./state"
import { rebuildAndBroadcast } from "./rebuild"
import { remapPositions } from "./remap"
import type { ServerWebSocket } from "flux:http"

// The control API under /__control__/: read-only introspection of connected
// app clients, served next to the file routes. The MCP bridge (srt mcp) is the
// primary consumer. Two shapes: server-held data answered directly (clients,
// logs) and queries forwarded to a client over its websocket and correlated
// back by id (tree, stats).

export type LogEntry = { seq: number; at: number; client: number; level: string; text: string }

// Ring buffer of forwarded client logs. Capped so a chatty app cannot grow the
// server without bound; readers page through it with the `since` cursor.
const LOG_CAP = 2000
const QUERY_TIMEOUT_MS = 5000
const MAX_WAIT_MS = 30000

let logs: LogEntry[] = []
let logSeq = 0
// Pending long-poll wakeups (see handleLogs). Flushed on every append; a
// waiter that already timed out resolves again harmlessly.
let logWaiters: Array<() => void> = []

let nextQueryId = 1
let pendingQueries = new Map<number, (msg: any) => void>()

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/// A `log` message arrived from a client: buffer it and wake long-polls.
/// Bundle positions in stack traces are remapped to .tsx sources on the way in.
export function appendLog(client: number, level: string, text: string) {
  logs.push({ seq: ++logSeq, at: Date.now(), client, level, text: remapPositions(text, state.currentMap) })
  if (logs.length > LOG_CAP) logs.splice(0, logs.length - LOG_CAP)
  let waiters = logWaiters
  logWaiters = []
  for (let wake of waiters) wake()
}

/// A `result` message arrived from a client: hand it to the awaiting query.
export function resolveQuery(msg: { id?: number }) {
  if (typeof msg.id !== "number") return
  let resolve = pendingQueries.get(msg.id)
  if (resolve) {
    pendingQueries.delete(msg.id)
    resolve(msg)
  }
}

// The connected-client list. `withAddress` adds each socket's peer address for
// the internal API (the repl `list` display); the public control shape stays
// without it.
export function clientList(withAddress = false) {
  return [...state.clients.entries()].map(([ws, info]) => ({
    id: info.id,
    platform: info.platform,
    version: info.version,
    profile: info.profile,
    capabilities: info.capabilities,
    ...(withAddress ? { address: ws.remoteAddress ?? null } : {}),
  }))
}

// Resolve the target client for a query: an explicit ?client=<id>, or the only
// connected client when the parameter is omitted.
function findClient(param: string | undefined): { ws: ServerWebSocket } | { error: Response } {
  let entries = [...state.clients.entries()]
  if (param === undefined) {
    if (entries.length === 1) return { ws: entries[0]![0] }
    if (entries.length === 0) return { error: Response.json({ error: "No connected clients" }, { status: 503 }) }
    return { error: Response.json({ error: "Multiple clients connected; pass ?client=<id>" }, { status: 400 }) }
  }
  let id = parseInt(param, 10)
  let entry = entries.find(([, info]) => info.id === id)
  if (!entry) {
    let ids = entries.map(([, info]) => info.id)
    return {
      error: Response.json(
        {
          error:
            `Client ${param} is gone (connected ids: ${ids.length ? ids.join(", ") : "none"}). ` +
            "Ids reset when the dev server restarts; call list_clients for current ones.",
        },
        { status: 404 },
      ),
    }
  }
  return { ws: entry[0] }
}

async function handleQuery(query: Map<string, string>, kind: string, extra?: Record<string, unknown>): Promise<Response> {
  let target = findClient(query.get("client"))
  if ("error" in target) return target.error
  let id = nextQueryId++
  let reply = new Promise<any>((resolve) => {
    pendingQueries.set(id, resolve)
  })
  target.ws.send(JSON.stringify({ type: "query", kind, id, ...extra }))
  let msg = await Promise.race([reply, sleep(QUERY_TIMEOUT_MS)])
  pendingQueries.delete(id)
  if (!msg)
    return Response.json(
      { error: "Query timed out: the client is connected but did not answer (JS thread busy or app wedged?)" },
      { status: 504 },
    )
  // Error strings may carry stack traces (e.g. a debug command threw); remap
  // bundle positions to .tsx sources like appendLog does for forwarded logs.
  if (msg.error) return Response.json({ error: remapPositions(String(msg.error), state.currentMap) }, { status: 502 })
  return Response.json(msg.data)
}

// Merge runs of consecutive identical entries (same client, level, text) into
// one entry carrying `repeats` and the run's last seq/at, so 176 copies of one
// error read as a single line and a `since` cursor still skips the whole run.
function collapseRepeats(entries: LogEntry[]): (LogEntry & { repeats?: number })[] {
  let out: (LogEntry & { repeats?: number })[] = []
  for (let e of entries) {
    let last = out[out.length - 1]
    if (last && last.client === e.client && last.level === e.level && last.text === e.text) {
      last.repeats = (last.repeats ?? 1) + 1
      last.seq = e.seq
      last.at = e.at
    } else {
      out.push({ ...e })
    }
  }
  return out
}

// GET /__control__/logs?since=N&wait=MS&level=L1,L2&contains=TEXT: entries with
// seq > since, plus the latest seq as the next cursor and the server
// generation. `level` keeps only the listed levels; `contains` keeps entries
// whose text has the substring (case-insensitive). Consecutive identical
// entries come back collapsed with a `repeats` count. With `wait`, holds the
// response until an entry passes the filters or the timeout expires
// (long-poll), so a caller can follow the stream without tight polling.
async function handleLogs(query: Map<string, string>): Promise<Response> {
  let since = parseInt(query.get("since") ?? "0", 10) || 0
  let wait = Math.min(parseInt(query.get("wait") ?? "0", 10) || 0, MAX_WAIT_MS)
  let levels = query
    .get("level")
    ?.split(",")
    .map((l) => l.trim())
    .filter(Boolean)
  let contains = query.get("contains")?.toLowerCase()
  let select = () =>
    logs.filter(
      (e) =>
        e.seq > since &&
        (!levels || levels.length === 0 || levels.includes(e.level)) &&
        (!contains || e.text.toLowerCase().includes(contains)),
    )
  let entries = select()
  // Filtered long-poll: an append may not pass the filters, so keep waiting
  // until one does or the deadline runs out.
  let deadline = Date.now() + wait
  while (entries.length === 0 && Date.now() < deadline) {
    await new Promise<void>((resolve) => {
      let timer = setTimeout(resolve, deadline - Date.now())
      logWaiters.push(() => {
        clearTimeout(timer)
        resolve()
      })
    })
    entries = select()
  }
  return Response.json({ entries: collapseRepeats(entries), latest: logSeq, generation: state.generation })
}

export async function handleControl(req: Request, path: string, query: Map<string, string>): Promise<Response> {
  switch (path) {
    case "/__control__/clients":
      return Response.json({ generation: state.generation, clients: clientList() })
    case "/__control__/logs":
      return handleLogs(query)
    case "/__control__/tree": {
      let extra: Record<string, unknown> = {}
      let root = parseInt(query.get("root") ?? "", 10)
      if (Number.isFinite(root)) extra.root = root
      let depth = parseInt(query.get("depth") ?? "", 10)
      if (Number.isFinite(depth)) extra.depth = depth
      let q = query.get("query")
      if (q) extra.query = q
      return handleQuery(query, "tree", extra)
    }
    case "/__control__/stats":
      return handleQuery(query, "stats")
    case "/__control__/snapshot": {
      let nodeId = parseInt(query.get("node") ?? "", 10)
      if (!Number.isFinite(nodeId)) return Response.json({ error: "Snapshot requires ?node=<id>" }, { status: 400 })
      return handleQuery(query, "snapshot", { nodeId })
    }
    case "/__control__/gpu":
      return handleQuery(query, "gpu")
    case "/__control__/debug": {
      // GET lists the app's registered debug commands; POST calls one, with
      // an optional JSON body as its args.
      if (req.method !== "POST") return handleQuery(query, "debug_list")
      let name = query.get("name")
      if (!name) return Response.json({ error: "Debug call requires ?name=<command>" }, { status: 400 })
      let args: unknown = null
      try {
        args = await req.json()
      } catch {}
      return handleQuery(query, "debug_call", { name, args })
    }
    case "/__control__/texture": {
      let textureId = parseInt(query.get("id") ?? "", 10)
      if (!Number.isFinite(textureId)) return Response.json({ error: "Texture requires ?id=<textureId>" }, { status: 400 })
      // Optional crop: all four of x/y/width/height, in texture pixels.
      let rectParams = ["x", "y", "width", "height"].map((k) => query.get(k))
      let extra: Record<string, unknown> = { textureId }
      if (rectParams.some((v) => v !== undefined)) {
        let [x, y, width, height] = rectParams.map((v) => parseInt(v ?? "", 10))
        if (![x, y, width, height].every(Number.isFinite))
          return Response.json({ error: "Texture rect requires all of x, y, width, height" }, { status: 400 })
        extra.rect = { x, y, width, height }
      }
      return handleQuery(query, "texture", extra)
    }
    case "/__control__/buffer": {
      let bufferId = parseInt(query.get("id") ?? "", 10)
      if (!Number.isFinite(bufferId)) return Response.json({ error: "Buffer requires ?id=<bufferId>" }, { status: 400 })
      let extra: Record<string, unknown> = { bufferId }
      let byteOffset = parseInt(query.get("offset") ?? "", 10)
      if (Number.isFinite(byteOffset)) extra.byteOffset = byteOffset
      let length = parseInt(query.get("length") ?? "", 10)
      if (Number.isFinite(length)) extra.length = length
      let as = query.get("as")
      if (as !== undefined) extra.as = as
      return handleQuery(query, "buffer", extra)
    }
    case "/__control__/reload": {
      // Explicit rebuild-and-push, the primary way a coding agent applies its
      // edits (srt mcp's reload tool). Unlike the repl's file watcher this is
      // on demand, so a burst of edits collapses into one reload.
      if (req.method !== "POST") return Response.json({ error: "Reload requires POST" }, { status: 405 })
      let error = await rebuildAndBroadcast()
      if (error) return Response.json({ error }, { status: 502 })
      state.watch = true
      return Response.json({ ok: true, clients: state.clients.size })
    }
    case "/__control__/load": {
      // Load (or switch) the app entry and push it: srt mcp's load tool.
      // Moves the rebuild entry and the file-serving root like the repl's
      // `load` command, then reuses the reload path, so a later /reload
      // rebuilds the newly loaded file. The srt process is not told: a
      // watcher started on the launch-time source keeps watching that file.
      if (req.method !== "POST") return Response.json({ error: "Load requires POST" }, { status: 405 })
      let entry = (await req.json().catch(() => null))?.entry
      if (typeof entry !== "string" || !entry) {
        return Response.json({ error: "Load requires { entry: <absolute source path> }" }, { status: 400 })
      }
      if (!(await file(entry).exists())) {
        return Response.json({ error: `Entry not found: ${entry}` }, { status: 400 })
      }
      state.config.entry = entry
      let cut = Math.max(entry.lastIndexOf("/"), entry.lastIndexOf("\\"))
      if (cut > 0) state.sourceDir = entry.slice(0, cut)
      let error = await rebuildAndBroadcast()
      if (error) return Response.json({ error }, { status: 502 })
      state.watch = true
      return Response.json({ ok: true, entry, clients: state.clients.size })
    }
    case "/__control__/watch": {
      // Pause/resume srt's auto-reload-on-save: the MCP watch tool. Latched
      // here because the watcher lives in the srt process; it reads the flag
      // via /__internal__/watch before acting on a change event. An agent
      // pauses while creating or editing files so half-finished work is not
      // pushed; a successful /reload or /load turns it back on.
      if (req.method !== "POST") return Response.json({ error: "Watch requires POST" }, { status: 405 })
      let enabled = (await req.json().catch(() => null))?.enabled
      if (typeof enabled !== "boolean") {
        return Response.json({ error: "Watch requires { enabled: <boolean> }" }, { status: 400 })
      }
      state.watch = enabled
      return Response.json({ ok: true, enabled })
    }
    default:
      return Response.json({ error: "Unknown control endpoint" }, { status: 404 })
  }
}
