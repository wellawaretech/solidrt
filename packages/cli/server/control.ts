import { state } from "./state"
import { rebuildAndBroadcast } from "./rebuild"
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
export function appendLog(client: number, level: string, text: string) {
  logs.push({ seq: ++logSeq, at: Date.now(), client, level, text })
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
  if (!entry) return { error: Response.json({ error: `No client with id ${param}` }, { status: 404 }) }
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
  if (!msg) return Response.json({ error: "Query timed out" }, { status: 504 })
  if (msg.error) return Response.json({ error: msg.error }, { status: 502 })
  return Response.json(msg.data)
}

// GET /__control__/logs?since=N&wait=MS: entries with seq > since, plus the
// latest seq as the next cursor. With `wait`, holds the response until a new
// entry arrives or the timeout passes (long-poll), so a caller can follow the
// stream without tight polling.
async function handleLogs(query: Map<string, string>): Promise<Response> {
  let since = parseInt(query.get("since") ?? "0", 10) || 0
  let wait = Math.min(parseInt(query.get("wait") ?? "0", 10) || 0, MAX_WAIT_MS)
  let entries = logs.filter((e) => e.seq > since)
  if (entries.length === 0 && wait > 0) {
    await new Promise<void>((resolve) => {
      let timer = setTimeout(resolve, wait)
      logWaiters.push(() => {
        clearTimeout(timer)
        resolve()
      })
    })
    entries = logs.filter((e) => e.seq > since)
  }
  return Response.json({ entries, latest: logSeq })
}

export async function handleControl(req: Request, path: string, query: Map<string, string>): Promise<Response> {
  switch (path) {
    case "/__control__/clients":
      return Response.json(clientList())
    case "/__control__/logs":
      return handleLogs(query)
    case "/__control__/tree":
      return handleQuery(query, "tree")
    case "/__control__/stats":
      return handleQuery(query, "stats")
    case "/__control__/snapshot": {
      let nodeId = parseInt(query.get("node") ?? "", 10)
      if (!Number.isFinite(nodeId)) return Response.json({ error: "Snapshot requires ?node=<id>" }, { status: 400 })
      return handleQuery(query, "snapshot", { nodeId })
    }
    case "/__control__/reload": {
      // Explicit rebuild-and-push, the primary way a coding agent applies its
      // edits (srt mcp's reload tool). Unlike the repl's file watcher this is
      // on demand, so a burst of edits collapses into one reload.
      if (req.method !== "POST") return Response.json({ error: "Reload requires POST" }, { status: 405 })
      let error = await rebuildAndBroadcast()
      if (error) return Response.json({ error }, { status: 502 })
      return Response.json({ ok: true, clients: state.clients.size })
    }
    default:
      return Response.json({ error: "Unknown control endpoint" }, { status: 404 })
  }
}
