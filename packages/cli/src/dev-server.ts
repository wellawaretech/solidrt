import { resolve } from "path"
import { stat as fsStat, readdir } from "node:fs/promises"
import { networkInterfaces } from "node:os"
import { Bonjour } from "bonjour-service"
import qrcode from "qrcode-generator"
import { state, print } from "./util"
import { values } from "./args"
import * as cache from "./cache"

export const DEV_HOST = "127.0.0.1"
export const DEV_PORT = 15194

// Dev-server WS protocol helpers: the reload message shape and the stop broadcast.
export function buildReload(payload: { code?: string | null; bytecode?: string }) {
  return JSON.stringify({ type: "reload", proxyFiles: values["proxy-files"], proxyHttp: values["proxy-http"], ...payload })
}

export function broadcastStop() {
  for (let ws of state.clients.keys()) {
    ws.send(JSON.stringify({ type: "stop" }))
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

      return new Response(Bun.file(filePath), { headers: { "X-SRT-Type": "file" } })
    },
    websocket: {
      open(ws) {
        state.clients.set(ws, { platform: "unknown", version: "unknown" })
        print(`[cli] Client connected ${ws.remoteAddress}`)
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
            state.clients.set(ws, {
              platform: data.platform ?? "unknown",
              version: data.version ?? "unknown",
            })
            print(`[cli] Client info ${ws.remoteAddress} ${data.platform} (${data.version})`)
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
  for (let y = 0; y < modCount; y += 2) {
    let row = "  "
    for (let x = 0; x < modCount; x++) {
      let top = qr.isDark(y, x)
      let bot = y + 1 < modCount && qr.isDark(y + 1, x)
      row += top && bot ? "\u2588" : top ? "\u2580" : bot ? "\u2584" : " "
    }
    console.log(row)
  }

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