import { state } from "./state"
import { rebuildAndBroadcast } from "./rebuild"
import { remapPositions } from "./remap"
import { ENTRY_EXTENSIONS, absolute, dirname } from "./mode"
import { file, realpath } from "flux:fs"
import type { ServerWebSocket } from "flux:http"
import type {
  ClientEntry,
  ClientsResponse,
  LoadResponse,
  LogEntry,
  LogsResponse,
  MuteResponse,
  ReloadResponse,
  WatchResponse,
} from "../types/control"

// The control API under /__control__/: read-only introspection of connected
// app clients, served next to the file routes. The MCP bridge (srt mcp) is the
// primary consumer. Two shapes: server-held data answered directly (clients,
// logs) and queries forwarded to a client over its websocket and correlated
// back by id (tree, stats).

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
  logs.push({ seq: ++logSeq, at: Date.now(), client, level, text: remapPositions(text, state.currentMaps) })
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
export function clientList(withAddress = false): (ClientEntry & { address?: string | null })[] {
  return [...state.clients.entries()].map(([ws, info]) => ({
    id: info.id,
    platform: info.platform,
    version: info.version,
    profile: info.profile,
    capabilities: info.capabilities,
    queries: info.queries,
    clientDir: info.clientDir,
    pid: info.pid,
    execPath: info.execPath,
    host: info.host,
    os: info.os,
    kernel: info.kernel,
    videoDriver: info.videoDriver,
    gpu: info.gpu,
    ...(withAddress ? { address: ws.remoteAddr ?? null } : {}),
  }))
}

// Resolve the target client for a query: an explicit ?client=<id>, or the only
// connected client when the parameter is omitted.
function findClient(param: string | undefined): { ws: ServerWebSocket } | { error: Response } {
  let entries = [...state.clients.entries()]
  if (param === undefined) {
    if (entries.length === 1) return { ws: entries[0]![0] }
    if (entries.length === 0) return { error: Response.json({ error: "No connected clients" }, { status: 503 }) }
    return { error: Response.json({ error: "Multiple clients connected; pass a client id (list_clients has the ids)" }, { status: 400 }) }
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

// Optional crop rect shared by /snapshot and /texture: all four of
// x/y/width/height in captured/texture pixels, or none. Returns undefined
// when absent, a 400 Response when malformed.
function parseRect(query: Map<string, string>): { x: number; y: number; width: number; height: number } | Response | undefined {
  let params = ["x", "y", "width", "height"].map((k) => query.get(k))
  if (params.every((v) => v === undefined)) return undefined
  let [x, y, width, height] = params.map((v) => parseInt(v ?? "", 10))
  if (![x, y, width, height].every(Number.isFinite))
    return Response.json({ error: "Crop rect requires all of x, y, width, height" }, { status: 400 })
  return { x: x!, y: y!, width: width!, height: height! }
}

// Optional output format shared by /snapshot and /texture: "png" (default,
// the reply carries pngBase64) or "raw" (rgbaBase64: RGBA8 bytes, rows
// top-down, no decoder needed for pixel assertions). Returns undefined for
// the default, or a 400 Response.
function parseFormat(query: Map<string, string>): "raw" | undefined | Response {
  let param = query.get("format")
  if (param === undefined || param === "png") return undefined
  if (param === "raw") return "raw"
  return Response.json({ error: 'Format must be "png" or "raw"' }, { status: 400 })
}

// Optional integer magnification shared by /snapshot and /texture. Returns
// undefined when absent or 1, a 400 Response when out of range.
function parseScale(query: Map<string, string>): number | Response | undefined {
  let param = query.get("scale")
  if (param === undefined) return undefined
  let scale = parseInt(param, 10)
  if (!Number.isFinite(scale) || scale < 1 || scale > 8)
    return Response.json({ error: "Scale must be an integer between 1 and 8" }, { status: 400 })
  return scale === 1 ? undefined : scale
}

async function handleQuery(
  query: Map<string, string>,
  kind: string,
  extra?: Record<string, unknown>,
  timeoutMs: number = QUERY_TIMEOUT_MS,
): Promise<Response> {
  let target = findClient(query.get("client"))
  if ("error" in target) return target.error
  let id = nextQueryId++
  let reply = new Promise<any>((resolve) => {
    pendingQueries.set(id, resolve)
  })
  target.ws.send(JSON.stringify({ type: "query", kind, id, ...extra }))
  let msg = await Promise.race([reply, sleep(timeoutMs)])
  pendingQueries.delete(id)
  if (!msg)
    return Response.json(
      { error: "Query timed out: the client is connected but did not answer (JS thread busy or app wedged?)" },
      { status: 504 },
    )
  // Error strings may carry stack traces (e.g. a debug command threw); remap
  // bundle positions to .tsx sources like appendLog does for forwarded logs.
  if (msg.error) return Response.json({ error: remapPositions(String(msg.error), state.currentMaps) }, { status: 502 })
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
  let body: LogsResponse = { entries: collapseRepeats(entries), latest: logSeq, generation: state.generation }
  return Response.json(body)
}

// Whether `path` lies under `root` (both canonical), never equal to it.
function isUnder(path: string, root: string): boolean {
  return path.length > root.length && path.startsWith(root) && (path[root.length] === "/" || path[root.length] === "\\")
}

// Load (or switch) the app entry and push it: the /load route (srt mcp's
// load tool) and the repl's `load`. Moves the rebuild entry, then reuses the
// reload path, so later reloads rebuild the new file. A project server stays
// inside its project (the bundle resolves the project's dependencies and
// assets, and the key keeps naming the project); a file server takes any
// file, moving the file routes and the bundler's cwd along with the entry.
// The key never moves: it names what the server was started for, and
// /clients reports the entry next to it. `status` is the HTTP status the
// error maps to (400 for a bad request, 502 for a failed build).
export async function loadEntry(requested: string): Promise<{ entry: string } | { error: string; status: number }> {
  if (!ENTRY_EXTENSIONS.some((ext) => requested.endsWith(ext))) {
    return { error: `Not an app entry: ${requested} (expected .tsx, .jsx, .ts, .js or .srt.js)`, status: 400 }
  }
  let config = state.config
  let path = absolute(requested, config.projectDir ?? config.sourceDir)
  if (!(await file(path).exists())) return { error: `Entry not found: ${path}`, status: 400 }
  let entry = await realpath(path)
  if (config.projectDir && !isUnder(entry, config.projectDir)) {
    return {
      error: `Entry is outside the project: ${entry} is not under ${config.projectDir}. A project server only bundles sources inside its project; start srt for that file on its own.`,
      status: 400,
    }
  }
  config.entry = entry
  config.entryArgs[0] = entry
  if (!config.projectDir) {
    config.sourceDir = dirname(entry)
    config.cwd = config.sourceDir
  }
  console.log(`[cli] Loading ${entry}`)
  let error = await rebuildAndBroadcast()
  if (error) return { error, status: 502 }
  return { entry }
}

// Mute or unmute the user's own input on every client (srt mcp's
// mute_user_input/unmute_user_input, the repl's `mute`): while muted, a
// measurement or an interaction test is not disturbed by a stray click;
// synthetic /input still goes through. Latched for clients joining while
// muted (the welcome message) and broadcast to the connected ones; no ack,
// the mute takes effect on arrival.
export function setUserInputMuted(on: boolean) {
  if (on !== state.userInputMuted) {
    console.log(on ? "[cli] User input muted on every client" : "[cli] User input unmuted")
  }
  state.userInputMuted = on
  let text = JSON.stringify({ type: "mute", active: on })
  for (let ws of state.clients.keys()) ws.send(text)
}

// Pause or resume reload-on-save (srt mcp's pause_watch/resume_watch, the
// repl's `watch`): paused, an agent's saves are not pushed while it edits;
// its explicit /reload is. Changes made while paused are not replayed on
// resume.
export function setWatchActive(on: boolean) {
  if (on === state.watchPaused) {
    console.log(on ? "[cli] Reload on save resumed" : "[cli] Reload on save paused")
  }
  state.watchPaused = !on
}

// Toggle the stats overlay on every client (the repl's `stats`); the welcome
// message carries it to clients joining later.
export function setStats(on: boolean) {
  state.stats = on
  let text = JSON.stringify({ type: "stats", stats: on })
  for (let ws of state.clients.keys()) ws.send(text)
}

export async function handleControl(req: Request, path: string, query: Map<string, string>): Promise<Response> {
  switch (path) {
    case "/__control__/clients":
      // `key`/`mode`/`entry` identify what this server serves, for a caller
      // that resolved it from the registry and wants to confirm the match.
      let body: ClientsResponse = {
        generation: state.generation,
        key: state.config.key,
        mode: state.config.mode,
        entry: state.config.entry,
        projectDir: state.config.projectDir,
        userInputMuted: state.userInputMuted,
        watchPaused: state.watchPaused,
        clients: clientList(),
      }
      return Response.json(body)
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
      if (query.get("props") === "true") extra.props = true
      return handleQuery(query, "tree", extra)
    }
    case "/__control__/stats": {
      let extra: Record<string, unknown> = {}
      let windowMs = parseInt(query.get("window") ?? "", 10)
      if (Number.isFinite(windowMs)) extra.windowMs = windowMs
      return handleQuery(query, "stats", extra)
    }
    case "/__control__/snapshot": {
      let nodeId = parseInt(query.get("node") ?? "", 10)
      if (!Number.isFinite(nodeId)) return Response.json({ error: "Snapshot requires ?node=<id>" }, { status: 400 })
      let extra: Record<string, unknown> = { nodeId }
      let rect = parseRect(query)
      if (rect instanceof Response) return rect
      if (rect) extra.rect = rect
      let scale = parseScale(query)
      if (scale instanceof Response) return scale
      if (scale) extra.scale = scale
      let format = parseFormat(query)
      if (format instanceof Response) return format
      if (format) extra.format = format
      return handleQuery(query, "snapshot", extra)
    }
    case "/__control__/gpu": {
      // ?label=<text> keeps only resources created with exactly that label.
      let label = query.get("label")
      return handleQuery(query, "gpu", label === undefined ? undefined : { label })
    }
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
      let extra: Record<string, unknown> = { textureId }
      let rect = parseRect(query)
      if (rect instanceof Response) return rect
      if (rect) extra.rect = rect
      let scale = parseScale(query)
      if (scale instanceof Response) return scale
      if (scale) extra.scale = scale
      let format = parseFormat(query)
      if (format instanceof Response) return format
      if (format) extra.format = format
      return handleQuery(query, "texture", extra)
    }
    case "/__control__/clock": {
      // Clock control: ?scale=<x> sets the client's time scale (0 pauses),
      // ?step=<n> advances n frames while paused. Applied by the client
      // runtime; the reply carries the resulting clock state.
      if (req.method !== "POST") return Response.json({ error: "Clock requires POST" }, { status: 405 })
      let extra: Record<string, unknown> = {}
      let scaleParam = query.get("scale")
      if (scaleParam !== undefined) {
        let scale = parseFloat(scaleParam)
        if (!Number.isFinite(scale) || scale < 0)
          return Response.json({ error: "Clock scale must be a number >= 0" }, { status: 400 })
        extra.scale = scale
      }
      let stepParam = query.get("step")
      if (stepParam !== undefined) {
        let step = parseInt(stepParam, 10)
        if (!Number.isFinite(step) || step < 1 || step > 1000)
          return Response.json({ error: "Clock step must be an integer between 1 and 1000" }, { status: 400 })
        extra.step = step
      }
      if (!("scale" in extra) && !("step" in extra))
        return Response.json({ error: "Clock requires ?scale=<x> or ?step=<n>" }, { status: 400 })
      return handleQuery(query, "clock", extra)
    }
    case "/__control__/input": {
      // Synthetic input injection: POST {events: [...]} forwards a timed
      // event sequence to the client, which feeds it through the real input
      // pipeline. Shape checks only here - the runtime validates each event
      // and rejects the whole sequence on any bad one. The query timeout
      // stretches by the sequence's own delays, since the client replies
      // only after the last event has been sent.
      if (req.method !== "POST") return Response.json({ error: "Input requires POST" }, { status: 405 })
      let body: any = null
      try {
        body = await req.json()
      } catch {}
      let events = body?.events
      if (!Array.isArray(events) || events.length === 0)
        return Response.json({ error: "Input requires a body {events: [...]} with at least one event" }, { status: 400 })
      if (events.length > 200) return Response.json({ error: "Input sequences are capped at 200 events" }, { status: 400 })
      let totalMs = 0
      for (let e of events) {
        if (typeof e !== "object" || e === null)
          return Response.json({ error: "Each event must be an object" }, { status: 400 })
        for (let f of ["delayMs", "holdMs"]) {
          let v = e[f]
          if (v !== undefined) {
            if (typeof v !== "number" || !Number.isInteger(v) || v < 0 || v > 5000)
              return Response.json({ error: `Event ${f} must be an integer between 0 and 5000` }, { status: 400 })
            totalMs += v
          }
        }
      }
      if (totalMs > 30000)
        return Response.json({ error: "Input sequence too long: delays and holds total over 30000 ms" }, { status: 400 })
      return handleQuery(query, "input", { events }, QUERY_TIMEOUT_MS + totalMs)
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
      // Explicit rebuild-and-push, the way a coding agent applies its edits
      // (srt mcp's reload tool): a burst of edits collapses into one reload,
      // with reload-on-save paused meanwhile (/watch).
      if (req.method !== "POST") return Response.json({ error: "Reload requires POST" }, { status: 405 })
      let error = await rebuildAndBroadcast()
      if (error) return Response.json({ error }, { status: 502 })
      let body: ReloadResponse = { ok: true, clients: state.clients.size }
      return Response.json(body)
    }
    case "/__control__/load": {
      if (req.method !== "POST") return Response.json({ error: "Load requires POST" }, { status: 405 })
      let requested = (await req.json().catch(() => null))?.entry
      if (typeof requested !== "string" || !requested) {
        return Response.json({ error: "Load requires { entry: <source path> }" }, { status: 400 })
      }
      let result = await loadEntry(requested)
      if ("error" in result) return Response.json({ error: result.error }, { status: result.status })
      let body: LoadResponse = { ok: true, entry: result.entry, clients: state.clients.size }
      return Response.json(body)
    }
    case "/__control__/mute": {
      if (req.method !== "POST") return Response.json({ error: "Mute requires POST" }, { status: 405 })
      let active = query.get("active")
      if (active !== "true" && active !== "false") {
        return Response.json({ error: "Mute requires ?active=true or ?active=false" }, { status: 400 })
      }
      let on = active === "true"
      setUserInputMuted(on)
      let body: MuteResponse = { ok: true, active: on, clients: state.clients.size }
      return Response.json(body)
    }
    case "/__control__/watch": {
      if (req.method !== "POST") return Response.json({ error: "Watch requires POST" }, { status: 405 })
      let active = query.get("active")
      if (active !== "true" && active !== "false") {
        return Response.json({ error: "Watch requires ?active=true or ?active=false" }, { status: 400 })
      }
      let on = active === "true"
      setWatchActive(on)
      let body: WatchResponse = { ok: true, active: on }
      return Response.json(body)
    }
    default:
      return Response.json({ error: "Unknown control endpoint" }, { status: 404 })
  }
}
