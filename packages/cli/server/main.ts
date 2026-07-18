// The srt dev server as a flux script. srt (Bun) spawns this with one JSON
// config argument; bundling, file watching, and the repl stay in srt, which
// drives this process over the loopback-only /__internal__/ routes. The
// shutdown-when-empty policy also lives in srt (it polls /__internal__/clients);
// this process runs until srt kills it. See docs/flux-dev-server-plan.md.

import { serve } from "flux:http"
import type { FluxRequest, Server } from "flux:http"
import { file, dir } from "flux:fs"
import { argv } from "flux:process"
import { resolveWithin, join } from "flux:path"
import { state, type Config } from "./state"
import * as cache from "./cache"
import { handleProxy } from "./proxy"
import { appendLog, clientList, handleControl, resolveQuery } from "./control"
import { printQr } from "./qr"
import { createTunnelEndpoint, TUNNEL_PROTOCOL } from "./tunnel"

// argv layout differs between hosts; the config JSON is always the last argument.
let config: Config = JSON.parse(argv[argv.length - 1]!)
state.config = config
state.sourceDir = config.sourceDir
state.stats = config.stats
state.serverUrl = `${config.address}:${config.port}`

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

// Send `text` to the clients with the given ids, or to every client when
// `ids` is omitted.
function sendTo(ids: number[] | undefined, text: string) {
  for (let [ws, info] of state.clients) {
    if (!ids || ids.includes(info.id)) ws.send(text)
  }
}

// The srt -> server IPC under /__internal__/: only the srt process on this
// machine may drive it, so reject any peer that is not loopback.
async function handleInternal(req: FluxRequest, server: Server, path: string): Promise<Response> {
  let ip = server.requestIP(req)
  let loopback = ip && (ip.address === "127.0.0.1" || ip.address === "::1" || ip.address === "::ffff:127.0.0.1")
  if (!loopback) return new Response("Forbidden", { status: 403 })

  if (path === "/__internal__/clients") return Response.json(clientList(true))
  if (path === "/__internal__/watch" && req.method === "GET") return Response.json({ enabled: state.watch })
  if (req.method !== "POST") return new Response("Method not allowed", { status: 405 })

  switch (path) {
    case "/__internal__/reload": {
      // { message, clients?, latch?, sourceDir?, map? }: send `message` (a full
      // client-protocol message, built by srt) to the listed client ids, or to
      // all when omitted. `latch` keeps it for late-joining clients (code
      // reloads latch, one-shot bytecode loads do not); `sourceDir` moves the
      // file-serving root (repl `load`); `map` is the bundle's sourcemap for
      // log remapping, replaced on every reload (absent means none).
      let body = await req.json()
      if (typeof body.sourceDir === "string") state.sourceDir = body.sourceDir
      // Keep the rebuild entry in sync when `load` moves it, so a later MCP
      // reload bundles the newly loaded file, not the launch-time one.
      if (typeof body.entry === "string") state.config.entry = body.entry
      state.currentMap = typeof body.map === "string" ? body.map : null
      let text = JSON.stringify(body.message)
      if (body.latch) state.currentReload = text
      sendTo(body.clients, text)
      return new Response("", { status: 204 })
    }
    case "/__internal__/stop": {
      let body = await req.json()
      // A broadcast stop also forgets the latched reload, so a client that
      // connects afterwards starts clean.
      if (!body.clients) {
        state.currentReload = null
        state.currentMap = null
      }
      sendTo(body.clients, JSON.stringify({ type: "stop" }))
      return new Response("", { status: 204 })
    }
    case "/__internal__/watch": {
      // The repl's `watch on|off`; agents use /__control__/watch instead.
      let body = await req.json()
      state.watch = !!body.enabled
      return new Response("", { status: 204 })
    }
    case "/__internal__/stats": {
      let body = await req.json()
      state.stats = !!body.stats
      sendTo(undefined, JSON.stringify({ type: "stats", stats: state.stats }))
      return new Response("", { status: 204 })
    }
    default:
      return Response.json({ error: "Unknown internal endpoint" }, { status: 404 })
  }
}

// The file routes: GET file (with single-range 206 support) or directory
// listing, PUT file write. All paths are contained in the source directory.
async function handleFiles(req: FluxRequest, path: string): Promise<Response> {
  let filePath = resolveWithin(state.sourceDir, "." + path)
  if (!filePath) {
    return new Response("Forbidden", { status: 403 })
  }

  if (req.method === "PUT") {
    console.log("[cli] put " + path)
    let bytes = await req.bytes()
    await file(filePath).write(bytes)
    return new Response("", { status: 204 })
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
    let dirents = await dir(filePath).entries()
    let entries = await Promise.all(
      dirents.map(async (d) => {
        let entry = { name: d.name, type: d.type === "directory" ? 2 : 1, size: 0, modified: 0 }
        if (d.type !== "directory") {
          try {
            let s = await file(join(filePath, d.name)).stat()
            entry.size = s.size
            entry.modified = Math.floor(s.mtime ?? 0)
          } catch {}
        }
        return entry
      }),
    )
    entries.sort((a, b) => a.name.localeCompare(b.name))
    return Response.json(entries, { headers: { "X-SRT-Type": "directory" } })
  }

  let baseHeaders: Record<string, string> = { "X-SRT-Type": "file", "Accept-Ranges": "bytes" }

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

// Ticket-paired clients connect through this endpoint; serve() accepts its
// connections directly alongside the TCP listener.
let tunnel = config.tunnel ? await createTunnelEndpoint(config.port, config.cacheDir) : null

serve({
  port: config.port,
  p2p: tunnel ? { endpoint: tunnel, protocol: TUNNEL_PROTOCOL } : undefined,
  async fetch(req, server) {
    if (server.upgrade(req)) return

    let { path, query } = splitQuery(req.url)

    if (path === "/__proxy__") {
      return handleProxy(req)
    }
    if (path.startsWith("/__control__/")) {
      return handleControl(req, path, query)
    }
    if (path.startsWith("/__internal__/")) {
      return handleInternal(req, server, path)
    }
    return handleFiles(req, path)
  },
  websocket: {
    open(ws) {
      let id = state.nextClientId++
      state.clients.set(ws, { platform: "unknown", version: "unknown", profile: "unknown", id, capabilities: [] })
      console.log(`[cli] Client connected ${ws.remoteAddress ?? "unknown"}`)
      // Advertise our real LAN address so clients dialed over a loopback hop
      // can show/remember the directly reachable address (see connection.rs).
      ws.send(
        JSON.stringify({ type: "welcome", address: state.serverUrl, stats: state.stats, capture: !!config.capture }),
      )
      if (state.currentReload) {
        ws.send(state.currentReload)
      }
    },
    close(ws) {
      let info = state.clients.get(ws)
      state.clients.delete(ws)
      console.log(`[cli] Client disconnected: ${info?.platform ?? "unknown"}`)
    },
    message(ws, msg) {
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
          })
          console.log(`[cli] Client info ${ws.remoteAddress ?? "unknown"} ${data.platform} (${data.version})`)
        } else if (data.type === "log") {
          // Forwarded console output / runtime errors from the client's
          // engine logger, buffered for the control API (see control.ts).
          // Not printed here: the local client already writes to this
          // terminal, so echoing would duplicate every line.
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
    },
  },
})

// One QR on screen: with the tunnel on, the ticket QR (printed by
// createTunnelEndpoint) is the pairing story and the address stays text-only;
// without it, the address QR is the scan target as before.
if (!config.tunnel) {
  console.log("")
  printQr(state.serverUrl)
  console.log("")
}
console.log(`[cli] WebSocket server on ws://${state.serverUrl}`)
// mDNS advertise is intentionally not implemented here: the p2p ticket is the
// cross-device connect story (see docs/flux-dev-server-plan.md).

// Keepalive
setInterval(() => {
  for (let ws of state.clients.keys()) {
    ws.ping()
  }
}, 5000)
