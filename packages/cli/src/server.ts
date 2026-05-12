import { resolve } from "path"
import { stat as fsStat, readdir } from "node:fs/promises"
import { networkInterfaces } from "node:os"
import { createSocket } from "node:dgram"
import qrcode from "qrcode-generator"
import { DEV_HOST, DEV_PORT, state, print } from "./util"

export function startServer() {
  state.server = Bun.serve({
    port: DEV_PORT,
    async fetch(req, server) {
      if (server.upgrade(req)) return

      let url = new URL(req.url)
      let path = decodeURIComponent(url.pathname)

      print("[http] get", path)

      let filePath = resolve(state.sourceDir, "." + path)
      if (!filePath.startsWith(state.sourceDir)) {
        return new Response("Forbidden", { status: 403 })
      }
      let stat
      try {
        stat = await fsStat(filePath)
      } catch {
        print("[http] file not found %s", path)
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
        return Response.json(entries)
      }

      return new Response(Bun.file(filePath))
    },
    websocket: {
      open(ws) {
        state.clients.set(ws, { platform: "unknown", version: "unknown" })
        print(`[dev] Client connected ${ws.remoteAddress}`)
        if (state.currentCode) {
          ws.send(JSON.stringify({ type: "reload", code: state.currentCode }))
        }
      },
      close(ws) {
        let info = state.clients.get(ws)
        state.clients.delete(ws)
        print(`[dev] Client disconnected: ${info?.platform ?? "unknown"}`)
        if (state.child && state.clients.size === 0 && state.child.exitCode !== null) {
          print("[dev] All clients disconnected, shutting down")
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
            print(`[dev] Client info ${ws.remoteAddress} ${data.platform} (${data.version})`)
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
  console.log(`[dev] WebSocket server on ws://${serverUrl}`)

  // UDP discovery
  let udp = createSocket("udp4")
  udp.on("message", (msg, rinfo) => {
    if (msg.toString() === "SRT_DISCOVER") {
      print(`[dev] Discovery request from ${rinfo.address}:${rinfo.port}`)
      udp.send("SRT_SERVER", rinfo.port, rinfo.address)
    }
  })
  udp.bind(DEV_PORT, () => {
    udp.setBroadcast(true)
    print("[dev] UDP discovery listener on port " + DEV_PORT)
  })

  // Keepalive
  setInterval(() => {
    for (let ws of state.clients.keys()) {
      ws.ping()
    }
  }, 5000)
}