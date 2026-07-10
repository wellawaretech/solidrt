import { resolve } from "path"
import { stat as fsStat, readdir } from "node:fs/promises"
import { appendFileSync } from "node:fs"
import { networkInterfaces } from "node:os"
import { Bonjour } from "bonjour-service"
import qrcode from "qrcode-generator"
import { state, print } from "./util"
import { values } from "./args"
import * as cache from "./cache"
import { appendLog, handleControl, resolveQuery } from "./control"

export const DEV_HOST = "127.0.0.1"
export const DEV_PORT = 0x8844

// Dev-server WS protocol helpers: the reload message shape and a broadcast to all clients.
export function buildReload(payload: { code?: string | null; bytecode?: string }) {
  return JSON.stringify({ type: "reload", proxyFiles: values["proxy-files"], proxyHttp: values["proxy-http"], ...payload })
}

export function broadcast(msg: object) {
  let text = JSON.stringify(msg)
  for (let ws of state.clients.keys()) {
    ws.send(text)
  }
}

// Reload code that fails to start the engine on purpose. The runtime treats a
// startup error like any app that never called render() and falls back to its
// baked-in BSOD screen, so a build that doesn't compile shows the BSOD instead
// of leaving the previous app frozen on screen.
const BSOD_TRIGGER = `throw new Error("SolidRT: build failed")`

// Called when a bundle fails to compile. Latches the BSOD trigger as the
// current code (so a client connecting after the failure gets it too) and
// pushes it to every connected client.
export function showBuildFailure() {
  state.currentCode = BSOD_TRIGGER
  let msg = buildReload({ code: BSOD_TRIGGER })
  for (let ws of state.clients.keys()) {
    ws.send(msg)
  }
}

function headersToObject(h: Headers): Record<string, string> {
  let out: Record<string, string> = {}
  h.forEach((v, k) => {
    out[k] = v
  })
  return out
}
async function handleProxy(req: Request): Promise<Response> {
  let target = req.headers.get("x-srt-proxy-url")
  if (!target) {
    return new Response("Missing X-SRT-Proxy-Url", { status: 400 })
  }

  let forwardHeaders = new Headers(req.headers)
  forwardHeaders.delete("host")
  forwardHeaders.delete("x-srt-proxy-url")
  forwardHeaders.delete("x-srt-cache")
  forwardHeaders.delete("content-length")

  let cacheStatus: cache.Decision = "skip"
  let cacheable = !cache.shouldConsider(req.method, req.headers).skip
  let bypass = cacheable && cache.isBypass(req.headers)

  if (cacheable && !bypass) {
    let hit = cache.get(req.method, target)
    if (hit) {
      print("[cli] proxy %s %s [cache hit]", req.method, target)
      let respHeaders = new Headers(hit.headers)
      respHeaders.set("x-srt-cache", "hit")
      // await new Promise(resolve => setTimeout(resolve, 1000))
      return new Response(hit.body, { status: hit.status, headers: respHeaders })
    }
  }

  let hasBody = req.method !== "GET" && req.method !== "HEAD"
  if (cacheable) {
    cacheStatus = bypass ? "bypass" : "miss"
    print("[cli] proxy %s %s [%s]", req.method, target, cacheStatus)
  } else {
    print("[cli] proxy %s %s", req.method, target)
  }

  try {
    let upstream = await fetch(target, {
      method: req.method,
      headers: forwardHeaders,
      body: hasBody ? await req.arrayBuffer() : undefined,
      redirect: "follow",
    })
    let respHeaders = new Headers(upstream.headers)
    respHeaders.delete("content-encoding")
    respHeaders.delete("transfer-encoding")

    let bodyBytes = new Uint8Array(await upstream.arrayBuffer())
    if (cacheable) {
      cache.put(
        req.method,
        target,
        upstream.status,
        headersToObject(respHeaders),
        bodyBytes,
      )
      respHeaders.set("x-srt-cache", cacheStatus)
    }
    return new Response(bodyBytes, {
      status: upstream.status,
      statusText: upstream.statusText,
      headers: respHeaders,
    })
  } catch (e) {
    print("[cli] proxy error %s: %s", target, String(e))
    return new Response(`Proxy error: ${String(e)}`, { status: 502 })
  }
}

export function startServer() {
  state.server = Bun.serve({
    port: DEV_PORT,
    async fetch(req, server) {
      if (server.upgrade(req)) return

      let url = new URL(req.url)
      let path = decodeURIComponent(url.pathname)

      if (path === "/__proxy__") {
        return handleProxy(req)
      }

      if (path.startsWith("/__control__/")) {
        return handleControl(req, path)
      }

      let filePath = resolve(state.sourceDir, "." + path)
      if (!filePath.startsWith(state.sourceDir)) {
        return new Response("Forbidden", { status: 403 })
      }

      if (req.method === "PUT") {
        print("[cli] put", path)
        let bytes = new Uint8Array(await req.arrayBuffer())
        await Bun.write(filePath, bytes)
        return new Response(null, { status: 204 })
      }

      print("[cli] get", path)

      let stat
      try {
        stat = await fsStat(filePath)
      } catch {
        print("[cli] file not found %s", path)
        return new Response("Not found", { status: 404 })
      }

      if (stat.isDirectory()) {
        let dirents = await readdir(filePath, { withFileTypes: true })
        let entries = await Promise.all(
          dirents.map(async (d) => {
            let entry = { name: d.name, type: d.isDirectory() ? 2 : 1, size: 0, modified: 0 }
            if (!d.isDirectory()) {
              try {
                let s = await fsStat(resolve(filePath, d.name))
                entry.size = s.size
                entry.modified = Math.floor(s.mtimeMs)
              } catch {}
            }
            return entry
          }),
        )
        entries.sort((a, b) => a.name.localeCompare(b.name))
        return Response.json(entries, { headers: { "X-SRT-Type": "directory" } })
      }

      let file = Bun.file(filePath)
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
          let n = parseInt(match[2], 10)
          start = isNaN(n) ? 0 : Math.max(0, size - n)
          end = size - 1
        } else {
          start = parseInt(match[1], 10)
          end = match[2] === "" ? size - 1 : Math.min(parseInt(match[2], 10), size - 1)
        }
        if (start > end || start >= size) {
          return new Response("Range not satisfiable", {
            status: 416,
            headers: { ...baseHeaders, "Content-Range": `bytes */${size}` },
          })
        }
        return new Response(file.slice(start, end + 1), {
          status: 206,
          headers: {
            ...baseHeaders,
            "Content-Range": `bytes ${start}-${end}/${size}`,
            "Content-Length": String(end - start + 1),
          },
        })
      }

      return new Response(file, { headers: baseHeaders })
    },
    websocket: {
      open(ws) {
        let id = state.nextClientId++
        state.clients.set(ws, { platform: "unknown", version: "unknown", id, capabilities: [] })
        print(`[cli] Client connected ${ws.remoteAddress}`)
        // Advertise our real LAN address so clients dialed over the adb loopback
        // can show/remember the directly reachable address (see connection.rs).
        ws.send(
          JSON.stringify({ type: "welcome", address: state.serverUrl, stats: state.stats, capture: !!state.capture }),
        )
        if (state.currentCode) {
          ws.send(buildReload({ code: state.currentCode }))
        }
      },
      close(ws) {
        let info = state.clients.get(ws)
        state.clients.delete(ws)
        print(`[cli] Client disconnected: ${info?.platform ?? "unknown"}`)
        if (state.child && state.clients.size === 0 && state.child.exitCode !== null) {
          print("[cli] All clients disconnected, shutting down")
          state.server?.stop()
          process.exit(0)
        }
      },
      message(ws, msg) {
        try {
          let data = JSON.parse(typeof msg === "string" ? msg : Buffer.from(msg).toString())
          if (data.type === "info") {
            let existing = state.clients.get(ws)
            state.clients.set(ws, {
              platform: data.platform ?? "unknown",
              version: data.version ?? "unknown",
              id: existing?.id ?? state.nextClientId++,
              capabilities: Array.isArray(data.capabilities) ? data.capabilities.map(String) : [],
            })
            print(`[cli] Client info ${ws.remoteAddress} ${data.platform} (${data.version})`)
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
          } else if (data.type === "capture" && state.capture) {
            let device = state.clients.get(ws)?.id ?? -1
            // Milliseconds, integer: Date.now() is already integer ms, so the
            // delta needs no rounding.
            let at = Date.now() - state.captureStartMs
            let after = at - state.captureLastAt
            state.captureLastAt = at
            // JSON Lines: one event object per line, streamed to disk as it
            // arrives rather than buffered - no in-memory growth for a long
            // capture, and the file is always complete on disk mid-session.
            let line = JSON.stringify({ after, type: data.kind, key: data.key, device }) + "\n"
            appendFileSync(state.capture, line)
          }
        } catch {}
      },
    },
  })

  let lanAddress = Object.values(networkInterfaces())
    .flat()
    .find((i) => i?.family === "IPv4" && !i.internal)?.address

  let address = lanAddress ?? DEV_HOST
  let serverUrl = `${address}:${state.server.port}`
  state.serverUrl = serverUrl

  console.log("")

  let qr = qrcode(0, "L")
  qr.addData(serverUrl)
  qr.make()
  let modCount = qr.getModuleCount()
  // Render as a white tile with black modules (explicit ANSI colors) plus the
  // 4-module quiet zone the QR spec requires. Drawing with the terminal's
  // default foreground inverts the code on dark themes, which standard
  // decoders reject.
  const QR_INK = "\x1b[30;107m" // black modules on bright-white tile (tile = background)
  const QR_TILE_FG = "\x1b[97m" // bright-white as foreground over the default background
  const QR_RESET = "\x1b[0m"
  const QUIET_ZONE = 2 // modules (spec says 4, but scanners cope and it reads tighter)
  let qrWidth = modCount + 2 * QUIET_ZONE
  let dark = (y: number, x: number) => y >= 0 && y < modCount && x >= 0 && x < modCount && qr.isDark(y, x)
  // modCount is always odd, so the tile is a half-line taller than an even row
  // count. The loop packs two module-rows per line via half-blocks and stops on
  // the last content row, leaving the bottom quiet zone half a line short of the
  // full-line top quiet zone.
  for (let y = -QUIET_ZONE; y < modCount + QUIET_ZONE - 1; y += 2) {
    let row = "  " + QR_INK
    for (let x = -QUIET_ZONE; x < modCount + QUIET_ZONE; x++) {
      let top = dark(y, x)
      let bot = dark(y + 1, x)
      row += top && bot ? "\u2588" : top ? "\u2580" : bot ? "\u2584" : " "
    }
    console.log(row + QR_RESET)
  }
  // Close that gap with a half-height tile line: upper half painted in the tile
  // color (foreground), lower half the terminal background. The 0.5 here plus
  // the 0.5 already under the last content row equal the full-line top margin.
  console.log("  " + QR_TILE_FG + "\u2580".repeat(qrWidth) + QR_RESET)

  console.log("")
  console.log(`[cli] WebSocket server on ws://${serverUrl}`)

  // LAN discovery: advertise the dev server as a DNS-SD service so go clients on
  // the same network can find it (see lattice/src/go/connection.rs). Stored on
  // state so shutdown() can send the mDNS goodbye.
  state.bonjour = new Bonjour()
  state.bonjour.publish({ name: "SolidRT Dev Server", type: "solidrt", protocol: "tcp", port: DEV_PORT })
  print(`[cli] Advertising _solidrt._tcp on port ${DEV_PORT} via mDNS`)

  // Keepalive
  setInterval(() => {
    for (let ws of state.clients.keys()) {
      ws.ping()
    }
  }, 5000)
}
