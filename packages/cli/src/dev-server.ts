import { resolve } from "path"
import { tmpdir, networkInterfaces } from "node:os"
import { fileURLToPath } from "node:url"
import { state, print, printErr, requireBinary, pipeAbovePrompt, shutdown } from "./util"
import { values } from "./args"

export const DEV_HOST = "127.0.0.1"
export const DEV_PORT = 0x8844

// The dev server itself is a flux script (packages/cli/server/), spawned by
// srt: bundling, file watching, and the repl stay here and drive the server
// process over its loopback-only /__internal__/ routes. See
// docs/flux-dev-server-plan.md.

const INTERNAL_BASE = `http://${DEV_HOST}:${DEV_PORT}/__internal__`

// Build the reload message for the client protocol. The server latches a
// broadcast reload verbatim for late-joining clients, so srt owns the message
// shape (including the proxy flags) end to end. `manifest` is the bundle's
// version manifest JSON string; when present, clients install the push into
// their version store before applying it (absent for bytecode one-shots and
// the BSOD trigger, which must not be installed).
export function buildReload(payload: { code?: string | null; bytecode?: string; manifest?: string | null }) {
  let { manifest, ...rest } = payload
  return {
    type: "reload",
    proxyHttp: values["proxy-http"],
    ...(manifest ? { manifest } : {}),
    ...rest,
  }
}

async function post(path: string, body: object) {
  let resp = await fetch(`${INTERNAL_BASE}${path}`, { method: "POST", body: JSON.stringify(body) })
  if (!resp.ok) throw new Error(`Dev server ${path} failed: ${resp.status}`)
}

/**
 * Send a client-protocol message through the server: to the given client ids,
 * or to every client when omitted. `latch` keeps the message for late-joining
 * clients (code reloads latch, one-shot bytecode loads do not); `sourceDir`
 * moves the server's file-serving root and `projectDir` its /assets/ root
 * (repl `load`); `map` is the bundle's sourcemap, kept server-side for
 * stack-trace remapping (omitting it clears the server's map, so a mapless
 * reload never remaps against a stale one).
 */
export async function sendReload(
  message: object,
  opts: {
    clients?: number[]
    latch?: boolean
    sourceDir?: string
    projectDir?: string
    entry?: string
    map?: string | null
  } = {},
) {
  await post("/reload", { message, ...opts })
}

/** Send a stop to the given client ids, or all. A broadcast stop clears the server's latched reload. */
export async function sendStop(clients?: number[]) {
  await post("/stop", clients ? { clients } : {})
}

/** Latch the stats-overlay flag on the server (for welcome) and broadcast it. */
export async function sendStats(stats: boolean) {
  await post("/stats", { stats })
}

/** Latch the auto-reload flag on the server (repl `watch on|off`). */
export async function sendWatch(enabled: boolean) {
  await post("/watch", { enabled })
}

/**
 * Whether the watcher may auto-reload: agents pause it via the MCP watch
 * tool, latched on the server. Fails open so an unreachable server surfaces
 * as a reload error, not a silently ignored change.
 */
export async function watchAllowed(): Promise<boolean> {
  try {
    let resp = await fetch(`${INTERNAL_BASE}/watch`)
    if (!resp.ok) return true
    let data = (await resp.json()) as { enabled?: boolean }
    return data.enabled !== false
  } catch {
    return true
  }
}

export type ClientEntry = {
  id: number
  platform: string
  version: string
  capabilities: string[]
  address: string | null
}

/** The connected-client list, in connect order. */
export async function getClients(): Promise<ClientEntry[]> {
  let resp = await fetch(`${INTERNAL_BASE}/clients`)
  if (!resp.ok) throw new Error(`Dev server /clients failed: ${resp.status}`)
  return resp.json() as Promise<ClientEntry[]>
}

// Reload code that fails to start the engine on purpose. The runtime treats a
// startup error like any app that never called render() and falls back to its
// baked-in BSOD screen, so a build that doesn't compile shows the BSOD instead
// of leaving the previous app frozen on screen.
const BSOD_TRIGGER = `throw new Error("SolidRT: build failed")`

// Called when a bundle fails to compile. Latches the BSOD trigger as the
// current code (so a client connecting after the failure gets it too) and
// pushes it to every connected client.
export async function showBuildFailure() {
  state.currentCode = BSOD_TRIGGER
  // No manifest: the BSOD trigger is not a version and must never be installed.
  state.currentManifest = null
  await sendReload(buildReload({ code: BSOD_TRIGGER }), { latch: true })
}

// The Bun-hosted server used to exit from its ws close handler once the
// spawned local client had exited and the last remote client disconnected.
// The server process cannot see the child, so the policy lives here: called
// after the local client exits, poll the client list and shut down when it
// empties.
export function shutdownWhenEmpty() {
  let timer = setInterval(async () => {
    let clients = await getClients().catch(() => null)
    if (clients && clients.length === 0) {
      clearInterval(timer)
      print("[cli] All clients disconnected, shutting down")
      shutdown()
    }
  }, 2000)
}

// Bundle the server script to one plain-JS file the flux binary can run. Bun
// is already the bundler; the browser target keeps node builtins out, and the
// flux: capability modules stay external (the runtime provides them).
async function bundleServer(): Promise<string> {
  let entry = fileURLToPath(new URL("../server/main.ts", import.meta.url))
  let outfile = resolve(tmpdir(), `srt-dev-server-${process.pid}.js`)
  let result = await Bun.build({
    entrypoints: [entry],
    target: "browser",
    format: "esm",
    external: ["flux:*"],
  })
  if (!result.success) {
    printErr("[cli] Failed to bundle the dev server:")
    for (let log of result.logs) printErr(String(log))
    process.exit(1)
  }
  await Bun.write(outfile, result.outputs[0]!)
  return outfile
}

export async function startServer() {
  let flux = requireBinary("flux")
  let script = await bundleServer()

  let lanAddress = Object.values(networkInterfaces())
    .flat()
    .find((i) => i?.family === "IPv4" && !i.internal)?.address
  let address = lanAddress ?? DEV_HOST
  // The address clients can reach us on; also the dev base URL the bundler
  // rewrites asset imports against. The server has no OS module, so srt
  // computes it and passes it down.
  state.serverUrl = `${address}:${DEV_PORT}`

  // How the server rebuilds on an MCP-triggered reload: it cannot call
  // Bun.build itself (it is a flux process), so it spawns srt's own bun on the
  // standalone bundle-cli entry. Both paths are known here at spawn time.
  let bundleCli = fileURLToPath(new URL("./bundle-cli.ts", import.meta.url))

  let config = {
    port: DEV_PORT,
    sourceDir: state.sourceDir,
    projectDir: state.projectDir,
    address,
    proxyHttp: values["proxy-http"],
    entry: state.source,
    minify: values.minify,
    bundlerCmd: [process.execPath, bundleCli],
    cache: values["proxy-http"],
    cacheDir: resolve(".srt-data"),
    keyDir: process.cwd(),
    capture: state.capture,
    stats: state.stats,
    tunnel: values.tunnel,
  }

  state.serverProc = Bun.spawn([flux, script, JSON.stringify(config)], {
    stdio: ["ignore", "pipe", "pipe"],
  })
  if (state.serverProc.stdout && typeof state.serverProc.stdout !== "number")
    pipeAbovePrompt(state.serverProc.stdout, process.stdout)
  if (state.serverProc.stderr && typeof state.serverProc.stderr !== "number")
    pipeAbovePrompt(state.serverProc.stderr, process.stderr)

  state.serverProc.exited.then((code) => {
    if (!state.shuttingDown) {
      printErr(`[cli] Dev server exited unexpectedly (${code})`)
      process.exit(1)
    }
  })

  // Wait until the server answers on the internal API before anything else
  // (the initial bundle needs the dev base URL, clients need the port bound).
  for (let i = 0; ; i++) {
    try {
      await getClients()
      break
    } catch {
      if (i >= 100) {
        printErr("[cli] Dev server did not start")
        process.exit(1)
      }
      await Bun.sleep(100)
    }
  }

  // mDNS advertise (dropped, code kept for future use - see
  // docs/flux-dev-server-plan.md): the p2p ticket is the cross-device connect
  // story now. If advertise returns, it belongs next to the server (a flux
  // capability), not here.
  //
  // import { Bonjour } from "bonjour-service"  (top of file)
  // state.bonjour = new Bonjour()
  // state.bonjour.publish({ name: "SolidRT Dev Server", type: "solidrt", protocol: "tcp", port: DEV_PORT })
  // print(`[cli] Advertising _solidrt._tcp on port ${DEV_PORT} via mDNS`)
}
