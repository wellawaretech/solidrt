import { resolveBinary } from "./artifacts"
import { existsSync } from "node:fs"
import { resolve } from "node:path"
import type { Interface as ReadlineInterface } from "node:readline"
import type { Server as BunServer } from "bun"

export let state = {
  clients: new Map<any, { platform: string; version: string }>(),
  currentCode: null as string | null,
  source: undefined as string | undefined,
  sourceDir: process.cwd(),
  child: null as ReturnType<typeof Bun.spawn> | null,
  server: null as BunServer<undefined> | null,
  serverUrl: null as string | null,
  rl: null as ReadlineInterface | null,
}

export function requireBinary(name: string) {
  let path = resolveBinary(name)
  if (path) return path
  console.error(`Could not find ${name} binary.`)
  console.error("Build from source: run make solidrt-go, then set SRT_HOME=<SolidRT project home>")
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
  process.exit(0)
}