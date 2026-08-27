// The srt dev server as a flux script, complete on its own: started as
// `flux server.js [flags]` from a project root (or with a file), by `srt run`
// / `srt server` (bun launchers that only resolve the binaries and spawn it)
// or by the console. It resolves its mode (mode.ts), owns everything with a
// lifetime - the port (bound here: remembered, given, or the first free one),
// the registry record, the local client, the latched bundle, the control API -
// and spawns the two things only bun can do, the bundle (`srt bundle --json`,
// rebuild.ts) and the startup typecheck (`srt check`), by command name
// (okf/done/srt-command-folders.md).

import { serve } from "flux:http"
import type { FluxRequest, Server, ServerWebSocket } from "flux:http"
import { file, realpath } from "flux:fs"
import { on as onSignal } from "flux:process"
import { join, resolveWithin } from "flux:path"
import { command } from "flux:subprocess"
import { interfaces, probe } from "flux:net"
import type { Child } from "flux:subprocess"
import { state } from "./state"
import { fail, parseArgs } from "./args"
import type { ServerConfig } from "./config"
import { absolute, resolveMode, sourceDirOf } from "./mode"
import { requireBinary, srtCommand } from "./binaries"
import * as cache from "./cache"
import { handleProxy } from "./proxy"
import { appendLog, handleControl, resolveQuery } from "./control"
import { printQr } from "./qr"
import { createTunnelEndpoint, TUNNEL_PROTOCOL } from "./tunnel"
import { rebuildAndBroadcast, showBuildFailure } from "./rebuild"
import { stopWatcher } from "./watcher"
import { startRepl } from "./repl"
import { devDir, pruneDeadRecords, rememberedPort, removeRecord, runningFor, serverDirFor, writeRecord } from "./registry"

let args = parseArgs()
let mode = await resolveMode(args)

// Records left behind by crashed servers are fossils; clear them first.
await pruneDeadRecords()

// One server per key: a second run in the same project (or on the same
// file) points at the running one instead of racing it.
let running = await runningFor(mode.key)
if (running) {
  fail(`A dev server already serves ${mode.key} on port ${running.port} (pid ${running.pid}). Stop it first, or attach a client with srt client.`)
}

let srt = srtCommand()
let runner = args.client !== null ? await requireBinary("solidrt-go") : null
let serverDir = await serverDirFor(mode.key)

// The LAN address (for --lan): the IPv4 of the interface holding the default
// route, which is the one other hosts reach; VPN and bridge interfaces (wg0,
// docker0) are up too, so first-up would announce them at random. Without a
// default route, the first up, non-loopback IPv4.
let lanAddress = args.lan ? lanAddressOf(interfaces()) : undefined
function lanAddressOf(ifaces: ReturnType<typeof interfaces>): string | undefined {
  let v4 = (list: typeof ifaces) => list.flatMap((i) => i.addrs).find((a) => a.family === "v4")?.ip
  let reachable = ifaces.filter((i) => i.up && !i.loopback)
  return v4(reachable.filter((i) => i.default)) ?? v4(reachable)
}

// Storage flags for the local client: dev client trees live under
// ~/.solidrt/clients/client<N>/ (an explicit --data-root wins), passed
// absolute because the client chdirs into its app sandbox at startup.
let clientArgs: string[] = []
if (args.client !== null) {
  clientArgs.push("--data-root", args.dataRoot ? absolute(args.dataRoot, await realpath(".")) : devDir("clients"))
  clientArgs.push("--client", String(args.client))
  if (args.size) clientArgs.push("--size", args.size)
}

let config: ServerConfig = {
  mode: mode.mode,
  key: mode.key,
  serverDir,
  entry: mode.entry,
  sourceDir: sourceDirOf(mode),
  projectDir: mode.projectDir,
  cwd: mode.projectDir ?? sourceDirOf(mode),
  entryArgs: [mode.entry, mode.mode === "project" ? "--project" : "--file"],
  srt,
  port: args.port,
  lan: args.lan,
  address: lanAddress ?? "127.0.0.1",
  proxyHttp: args.proxyHttp,
  args: args.appArgs,
  minify: args.minify,
  cache: args.proxyHttp,
  // Build outputs and the proxy cache: the project's .srt-data, or the
  // server folder for a file served on its own (nothing else owns it).
  cacheDir: mode.projectDir ? join(mode.projectDir, ".srt-data") : join(serverDir, "data"),
  capture: args.capture,
  stats: args.stats,
  tunnel: args.tunnel,
  client: runner ? { cmd: runner, args: clientArgs } : null,
}
state.config = config
state.stats = config.stats

if (config.cache) {
  await cache.initCache({ dir: config.cacheDir })
  console.log("[cli] HTTP cache enabled")
}

if (config.capture) {
  // Start each capture from an empty file: appends would otherwise tack onto
  // whatever a previous run left behind.
  await file(config.capture).write("")
  state.captureStartMs = Date.now()
}

// Split an origin-form request URL ("/path?a=1&b=2") into its decoded path and
// query parameters. flux has no URL global; this covers what the routes need.
function splitQuery(url: string): { path: string; query: Map<string, string> } {
  let i = url.indexOf("?")
  let path = i < 0 ? url : url.slice(0, i)
  let query = new Map<string, string>()
  if (i >= 0) {
    for (let pair of url.slice(i + 1).split("&")) {
      if (!pair) continue
      let j = pair.indexOf("=")
      let k = j < 0 ? pair : pair.slice(0, j)
      let v = j < 0 ? "" : pair.slice(j + 1)
      query.set(decodeURIComponent(k), decodeURIComponent(v.replace(/\+/g, " ")))
    }
  }
  return { path: decodeURIComponent(path), query }
}

// The file routes: GET file with single-range 206 support. All paths are
// contained in `root` (the source directory, or the project dir for the
// /assets/ convention route).
async function handleFiles(req: FluxRequest, path: string, root: string): Promise<Response> {
  let filePath = resolveWithin(root, "." + path)
  if (!filePath) {
    return new Response("Forbidden", { status: 403 })
  }

  console.log("[cli] get " + path)

  let stat
  try {
    stat = await file(filePath).stat()
  } catch {
    console.log(`[cli] file not found ${path}`)
    return new Response("Not found", { status: 404 })
  }
  if (stat.type === "directory") {
    return new Response("Not found", { status: 404 })
  }

  let baseHeaders: Record<string, string> = { "Accept-Ranges": "bytes" }

  // Honor a single byte-range request (e.g. streaming audio decoding on the
  // client, which seeks and reads on demand). Only the common "bytes=a-b" /
  // "bytes=a-" / "bytes=-n" forms; anything else falls through to the whole
  // file. Range makes proxied streaming viable without pulling the whole
  // track over the wire.
  let range = req.headers.get("range")
  let match = range ? /^bytes=(\d*)-(\d*)$/.exec(range.trim()) : null
  if (match) {
    let size = stat.size
    let start: number
    let end: number
    if (match[1] === "") {
      // Suffix range: the last N bytes.
      let n = parseInt(match[2]!, 10)
      start = isNaN(n) ? 0 : Math.max(0, size - n)
      end = size - 1
    } else {
      start = parseInt(match[1]!, 10)
      end = match[2] === "" ? size - 1 : Math.min(parseInt(match[2]!, 10), size - 1)
    }
    if (start > end || start >= size) {
      return new Response("Range not satisfiable", {
        status: 416,
        headers: { ...baseHeaders, "Content-Range": `bytes */${size}` },
      })
    }
    return new Response(await file(filePath).read(start, end - start + 1), {
      status: 206,
      headers: {
        ...baseHeaders,
        "Content-Range": `bytes ${start}-${end}/${size}`,
        "Content-Length": String(end - start + 1),
      },
    })
  }

  return new Response(await file(filePath).bytes(), { headers: baseHeaders })
}

async function handleRequest(req: FluxRequest, server: Server): Promise<Response | undefined> {
  if (server.upgrade(req)) return

  let { path, query } = splitQuery(req.url)

  if (path === "/__proxy__") {
    return handleProxy(req)
  }
  if (path.startsWith("/__control__/")) {
    // Every control response names the key this server serves, so a caller
    // that resolved the port from the registry can confirm it reached the
    // server it meant.
    let resp = await handleControl(req, path, query)
    resp.headers.set("x-solidrt-project", config.key)
    resp.headers.set("x-solidrt-generation", String(state.generation))
    return resp
  }
  // The assets/ convention roots at the project dir (package.json), which is
  // not necessarily the entry's dir the file routes serve; clients fetch
  // manifest asset paths here (live proxy reads and store installs alike).
  // File mode has no project and so no assets.
  if (path === "/assets" || path.startsWith("/assets/")) {
    if (!config.projectDir) return new Response("Not found", { status: 404 })
    return handleFiles(req, path, config.projectDir)
  }
  // Isolate bundles are build outputs, not project files: the rebuild writes
  // them under <cacheDir>/isolates/, and the manifest lists them as
  // isolates/<id>.js.
  if (path.startsWith("/isolates/")) {
    return handleFiles(req, path, config.cacheDir)
  }
  return handleFiles(req, path, config.sourceDir)
}

// A string field of a client's `info`, or null when absent or malformed
// (an older runtime, or one without the fact).
function text(value: unknown): string | null {
  return typeof value === "string" ? value : null
}

function onOpen(ws: ServerWebSocket) {
  let id = state.nextClientId++
  state.clients.set(ws, {
    platform: "unknown",
    version: "unknown",
    profile: "unknown",
    id,
    capabilities: [],
    queries: [],
    stats: state.stats,
    timeScale: 1,
    clientDir: null,
    pid: null,
    execPath: null,
    host: null,
    os: null,
    kernel: null,
    videoDriver: null,
    gpu: null,
  })
  console.log(`[cli] Client connected ${ws.remoteAddr ?? "unknown"}`)
  // Advertise the address we are reachable on, so clients dialed over a
  // loopback hop can show/remember it (see connection.rs).
  ws.send(JSON.stringify({ type: "welcome", address: state.serverUrl, stats: state.stats, capture: !!config.capture, mute: state.userInputMuted }))
  if (state.currentReload) {
    ws.send(state.currentReload)
  }
}

function onClose(ws: ServerWebSocket) {
  let info = state.clients.get(ws)
  state.clients.delete(ws)
  console.log(`[cli] Client disconnected: ${info?.platform ?? "unknown"}`)
  // `srt run` lives as long as its clients: once the local client is gone,
  // the last remote disconnect ends the server. `srt server` runs until
  // stopped.
  if (config.client && localClientExited && state.clients.size === 0) shutdown()
}

function onMessage(ws: ServerWebSocket, msg: string | Uint8Array) {
  try {
    let data = JSON.parse(typeof msg === "string" ? msg : new TextDecoder().decode(msg))
    if (data.type === "info") {
      let existing = state.clients.get(ws)
      state.clients.set(ws, {
        platform: data.platform ?? "unknown",
        version: data.version ?? "unknown",
        profile: data.profile ?? "unknown",
        id: existing?.id ?? state.nextClientId++,
        capabilities: Array.isArray(data.capabilities) ? data.capabilities.map(String) : [],
        queries: Array.isArray(data.queries) ? data.queries.map(String) : [],
        stats: existing?.stats ?? state.stats,
        timeScale: existing?.timeScale ?? 1,
        clientDir: text(data.clientDir),
        pid: typeof data.pid === "number" ? data.pid : null,
        execPath: text(data.execPath),
        host: text(data.host),
        os: text(data.os),
        kernel: text(data.kernel),
        videoDriver: text(data.videoDriver),
        gpu:
          data.gpu && typeof data.gpu === "object"
            ? { vendor: text(data.gpu.vendor) ?? "", renderer: text(data.gpu.renderer) ?? "", version: text(data.gpu.version) ?? "" }
            : null,
      })
      console.log(`[cli] Client info ${ws.remoteAddr ?? "unknown"} ${data.platform} (${data.version})`)
    } else if (data.type === "log") {
      // Forwarded console output / runtime errors from the client's engine
      // logger, buffered for the control API (see control.ts). Not printed
      // here: the local client already writes to this terminal, so echoing
      // would duplicate every line.
      let device = state.clients.get(ws)?.id ?? -1
      appendLog(device, String(data.level ?? "log"), String(data.text ?? ""))
    } else if (data.type === "result") {
      // Reply to a query the control API forwarded to this client.
      resolveQuery(data)
    } else if (data.type === "capture" && config.capture) {
      let device = state.clients.get(ws)?.id ?? -1
      // Milliseconds, integer: Date.now() is already integer ms, so the
      // delta needs no rounding.
      let at = Date.now() - state.captureStartMs
      let after = at - state.captureLastAt
      state.captureLastAt = at
      // JSON Lines: one event object per line, streamed to disk as it
      // arrives rather than buffered - no in-memory growth for a long
      // capture, and the file is always complete on disk mid-session.
      // Appends are chained so events land in arrival order.
      let line = JSON.stringify({ after, type: data.kind, key: data.key, device }) + "\n"
      state.captureChain = state.captureChain.then(() => file(config.capture!).append(line))
    }
  } catch {}
}

// The port: an explicit --port, else the one this server bound last time
// (so a project keeps its port in practice), else the first free one from
// DEFAULT_PORT upward, so servers on one machine read as 34884, 34885, ...
// A remembered or default port that is taken is skipped; an explicit one
// that is taken is the user's problem to see.
const DEFAULT_PORT = 0x8844
const PORT_TRIES = 100
let host = config.lan ? "0.0.0.0" : "127.0.0.1"
let remembered = config.port ?? (await rememberedPort(config.serverDir))

// Ticket-paired clients connect through this endpoint; serve() accepts its
// connections directly alongside the TCP listener. Its UDP port follows the
// remembered port so a ticket stays stable across restarts.
let tunnel = config.tunnel ? await createTunnelEndpoint(remembered, config.serverDir) : null

function bind(port: number): Server {
  return serve({
    host,
    port,
    p2p: tunnel ? { endpoint: tunnel, protocol: TUNNEL_PROTOCOL } : undefined,
    fetch: handleRequest,
    websocket: { open: onOpen, close: onClose, message: onMessage },
  })
}

// A bind alone does not prove a port free: with SO_REUSEADDR (the default
// on a listener) Linux lets a loopback bind coexist with another process's
// all-interfaces listener on the same port, and the newcomer then silently
// takes the loopback traffic. So each candidate is dialed first; only a
// refusal means free.
async function bindFirstFree(): Promise<Server> {
  if (config.port !== undefined) return bind(config.port)
  let candidates: number[] = remembered !== null ? [remembered] : []
  for (let p = DEFAULT_PORT; p < DEFAULT_PORT + PORT_TRIES; p++) candidates.push(p)
  let last: unknown = null
  for (let port of candidates) {
    if ((await probe("127.0.0.1", port, { timeoutMs: 200 })) !== "closed") continue
    try {
      return bind(port)
    } catch (e) {
      last = e
    }
  }
  throw last ?? new Error(`No free port between ${DEFAULT_PORT} and ${DEFAULT_PORT + PORT_TRIES - 1}`)
}

let server = await bindFirstFree()

let address = config.lan ? config.address : "127.0.0.1"
state.serverUrl = `${address}:${server.port}`
await writeRecord(config, server.port, address)

// One QR on screen: with the tunnel on, the ticket QR (printed by
// createTunnelEndpoint) is the pairing story and the address stays text-only;
// on the LAN without it, the address QR is the scan target. Loopback-only has
// nothing to scan.
if (config.lan && !config.tunnel) {
  console.log("")
  printQr(state.serverUrl)
  console.log("")
}
console.log(`[cli] Dev server on http://${state.serverUrl} serving ${config.mode} ${config.key}`)

// Keepalive
let keepalive = setInterval(() => {
  for (let ws of state.clients.keys()) {
    ws.ping()
  }
}, 5000)

let shuttingDown = false
let stopRepl = () => {}
let localClient: Child | null = null
let localClientExited = false
let signalOffs = ["SIGINT", "SIGTERM"].map((signal) =>
  onSignal(signal, () => {
    shutdown()
  }),
)

// Orderly exit: drop the record, stop the client, close the listeners and
// release every handle that keeps the loop alive, so the process ends on
// its own (flux has no exit call; an idle loop is the exit).
async function shutdown() {
  if (shuttingDown) return
  shuttingDown = true
  clearInterval(keepalive)
  for (let off of signalOffs) off()
  stopRepl()
  stopWatcher()
  await removeRecord(config.serverDir)
  if (localClient) localClient.kill()
  server.close()
  if (tunnel) await tunnel.close()
}

// Print a child's output line by line as it arrives.
async function pump(stream: AsyncIterable<Uint8Array>, print: (line: string) => void) {
  let decoder = new TextDecoder()
  let rest = ""
  for await (let chunk of stream) {
    rest += decoder.decode(chunk, { stream: true })
    let lines = rest.split("\n")
    rest = lines.pop() ?? ""
    for (let line of lines) print(line)
  }
  if (rest) print(rest)
}

// The initial bundle, latched for the clients about to connect. A failed
// build shows the BSOD rather than nothing; the next reload retries. The
// rebuild arms reload-on-save from the bundle's inputs (watcher.ts).
console.log("[cli] Bundling (development)")
let buildError = await rebuildAndBroadcast()
if (buildError) {
  console.error(buildError)
  showBuildFailure()
}
console.log("[cli] Reload on save is on (pause it with the MCP pause_watch tool)")
stopRepl = startRepl(shutdown)

// Startup typecheck (`srt check <entry>`), deliberately not awaited: the
// report prints when tsc finishes, and a type error never gates the boot
// (srt check is the hard gate). Once per server lifetime; reloads never
// typecheck. A prebuilt .srt.js has no checkable program.
if (!config.entry.endsWith(".srt.js")) {
  let check = command(config.srt[0]!, [...config.srt.slice(1), "check", config.entry], { cwd: config.cwd }).spawn()
  pump(check.stdout, (line) => console.log(line))
  pump(check.stderr, (line) => console.error(line))
}

if (config.client) {
  // The local client dials the port bound above; srt could not know it.
  let child = command(config.client.cmd, [...config.client.args, "--dev-server", `127.0.0.1:${server.port}`]).spawn()
  localClient = child
  pump(child.stdout, (line) => console.log(line))
  pump(child.stderr, (line) => console.error(line))
  child.status().then(() => {
    localClient = null
    localClientExited = true
    if (shuttingDown) return
    if (state.clients.size === 0) {
      shutdown()
    } else {
      console.log(`[cli] Local client exited, ${state.clients.size} remote client(s) still connected`)
    }
  })
}
