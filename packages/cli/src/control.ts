import { state } from "./util"

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

function clientList() {
  return [...state.clients.values()].map((info) => ({
    id: info.id,
    platform: info.platform,
    version: info.version,
    capabilities: info.capabilities,
  }))
}

// Resolve the target client for a query: an explicit ?client=<id>, or the only
// connected client when the parameter is omitted.
function findClient(param: string | null): { ws: any } | { error: Response } {
  let entries = [...state.clients.entries()]
  if (param === null) {
    if (entries.length === 1) return { ws: entries[0]![0] }
    if (entries.length === 0) return { error: Response.json({ error: "No connected clients" }, { status: 503 }) }
    return { error: Response.json({ error: "Multiple clients connected; pass ?client=<id>" }, { status: 400 }) }
  }
  let id = parseInt(param, 10)
  let entry = entries.find(([, info]) => info.id === id)
  if (!entry) return { error: Response.json({ error: `No client with id ${param}` }, { status: 404 }) }
  return { ws: entry[0] }
}

async function handleQuery(url: URL, kind: string): Promise<Response> {
  let target = findClient(url.searchParams.get("client"))
  if ("error" in target) return target.error
  let id = nextQueryId++
  let reply = new Promise<any>((resolve) => {
    pendingQueries.set(id, resolve)
  })
  target.ws.send(JSON.stringify({ type: "query", kind, id }))
  let msg = await Promise.race([reply, Bun.sleep(QUERY_TIMEOUT_MS)])
  pendingQueries.delete(id)
  if (!msg) return Response.json({ error: "Query timed out" }, { status: 504 })
  if (msg.error) return Response.json({ error: msg.error }, { status: 502 })
  return Response.json(msg.data)
}

// GET /__control__/logs?since=N&wait=MS: entries with seq > since, plus the
// latest seq as the next cursor. With `wait`, holds the response until a new
// entry arrives or the timeout passes (long-poll), so a caller can follow the
// stream without tight polling.
async function handleLogs(url: URL): Promise<Response> {
  let since = parseInt(url.searchParams.get("since") ?? "0", 10) || 0
  let wait = Math.min(parseInt(url.searchParams.get("wait") ?? "0", 10) || 0, MAX_WAIT_MS)
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

export async function handleControl(req: Request, path: string): Promise<Response> {
  let url = new URL(req.url)
  switch (path) {
    case "/__control__/clients":
      return Response.json(clientList())
    case "/__control__/logs":
      return handleLogs(url)
    case "/__control__/tree":
      return handleQuery(url, "tree")
    case "/__control__/stats":
      return handleQuery(url, "stats")
    default:
      return Response.json({ error: "Unknown control endpoint" }, { status: 404 })
  }
}
