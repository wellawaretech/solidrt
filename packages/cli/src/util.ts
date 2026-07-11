import { resolveBinary } from "./artifacts"
import { existsSync } from "node:fs"
import { resolve } from "node:path"
import type { Interface as ReadlineInterface } from "node:readline"
// import type { Bonjour } from "bonjour-service"  (mDNS advertise, kept - see dev-server.ts)

export let state = {
  // What srt believes the current bundle is; the server process keeps its own
  // latched copy for late-joining clients (see packages/cli/server/).
  currentCode: null as string | null,
  source: undefined as string | undefined,
  sourceDir: process.cwd(),
  child: null as ReturnType<typeof Bun.spawn> | null,
  // The spawned flux dev-server process (see dev-server.ts startServer).
  serverProc: null as ReturnType<typeof Bun.spawn> | null,
  shuttingDown: false,
  serverUrl: null as string | null,
  rl: null as ReadlineInterface | null,
  // bonjour: null as Bonjour | null,  (mDNS advertise, kept - see dev-server.ts)
  stats: false,
  // --capture <file>: destination for captured key events, or undefined when
  // off; the server process owns the capture file and clock (see
  // packages/cli/server/main.ts's "capture" message handling).
  capture: undefined as string | undefined,
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

// Pipe a child stream to `out` without mangling the repl prompt: clear the
// prompt line, write the chunk, redraw the prompt.
export function pipeAbovePrompt(stream: ReadableStream<Uint8Array>, out: NodeJS.WriteStream) {
  let reader = stream.getReader()
  ;(async () => {
    while (true) {
      let { done, value } = await reader.read()
      if (done || !value) break
      process.stdout.write("\r\x1b[K")
      out.write(value)
      state.rl?.prompt(true)
    }
  })()
}

export function shutdown() {
  state.shuttingDown = true
  if (state.child) state.child.kill()
  if (state.serverProc) state.serverProc.kill()
  // if (state.bonjour) state.bonjour.destroy()  (mDNS advertise, kept - see dev-server.ts)
  process.exit(0)
}