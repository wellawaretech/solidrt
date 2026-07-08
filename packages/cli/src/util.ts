import { resolveBinary } from "./artifacts"
import { existsSync } from "node:fs"
import { resolve } from "node:path"
import type { Interface as ReadlineInterface } from "node:readline"
import type { Server as BunServer } from "bun"
import type { Bonjour } from "bonjour-service"

export let state = {
  clients: new Map<any, { platform: string; version: string; id: number }>(),
  nextClientId: 0,
  currentCode: null as string | null,
  source: undefined as string | undefined,
  sourceDir: process.cwd(),
  child: null as ReturnType<typeof Bun.spawn> | null,
  server: null as BunServer<undefined> | null,
  serverUrl: null as string | null,
  rl: null as ReadlineInterface | null,
  bonjour: null as Bonjour | null,
  stats: false,
  // --capture <file>: destination for captured key events, or undefined when
  // off. Clients only report kind/key; the server stamps `after` itself (one
  // shared clock from captureStartMs, integer milliseconds) so events from
  // several connected clients merge into one coherent timeline, tagged by
  // `device` (see dev-server.ts) so they can be told apart. Streamed to disk
  // as JSON Lines (one event object per line) as each arrives - see
  // dev-server.ts's "capture" message handling.
  capture: undefined as string | undefined,
  captureStartMs: 0,
  captureLastAt: 0, // ms, same clock as captureStartMs
}

// Build target per binary, for the "not found" hint. Run from the repo root.
let BUILD_HINTS: Record<string, string> = {
  "solidrt-go": "make client",
  solidrt: "make runtime",
  flux: "make -C flux flux",
  fluxc: "make -C flux fluxc",
  fluxrt: "make -C flux fluxrt PROFILE=release-opt",
}

export function requireBinary(name: string) {
  let path = resolveBinary(name)
  if (path) return path
  let hint = BUILD_HINTS[name]
  console.error(`Could not find ${name} binary.`)
  if (hint) {
    console.error(`Build it from source: run ${hint}, with SRT_HOME pointing at your SolidRT checkout.`)
  } else {
    console.error("Build it from source, with SRT_HOME pointing at your SolidRT checkout.")
  }
  process.exit(1)
}

// adb is a system tool (Android Platform Tools), never bundled. Look on PATH
// first, then the standard SDK location.
export function resolveAdb() {
  let exe = process.platform === "win32" ? "adb.exe" : "adb"
  let onPath = Bun.which(exe)
  if (onPath) return onPath
  for (let root of [process.env.ANDROID_HOME, process.env.ANDROID_SDK_ROOT]) {
    if (!root) continue
    let candidate = resolve(root, "platform-tools", exe)
    if (existsSync(candidate)) return candidate
  }
  return null
}

export function requireAdb() {
  let path = resolveAdb()
  if (path) return path
  console.error("Could not find adb (Android Platform Tools).")
  console.error("Install it:")
  console.error("  Windows: winget install Google.PlatformTools")
  console.error("  macOS:   brew install android-platform-tools")
  console.error("  Linux:   install your distro's android-tools / adb package")
  process.exit(1)
}

export async function run(binary: string, args: string[]) {
  let proc = Bun.spawn([binary, ...args], { stdio: ["inherit", "inherit", "inherit"] })
  return proc.exited
}

export function print(...args: any[]) {
  process.stdout.write("\r\x1b[K")
  console.log(...args)
  state.rl?.prompt(true)
}

export function printErr(...args: any[]) {
  process.stdout.write("\r\x1b[K")
  console.error(...args)
  state.rl?.prompt(true)
}

export function shutdown() {
  if (state.child) state.child.kill()
  if (state.server) state.server.stop()
  if (state.bonjour) state.bonjour.destroy()
  process.exit(0)
}