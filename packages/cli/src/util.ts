import { resolveBinary } from "./native"
import type { Interface as ReadlineInterface } from "node:readline"
import type { Server as BunServer } from "bun"

export const DEV_HOST = "127.0.0.1"
export const DEV_PORT = 15194

export let state = {
  clients: new Map<any, { platform: string; version: string }>(),
  currentCode: null as string | null,
  source: undefined as string | undefined,
  sourceDir: process.cwd(),
  child: null as ReturnType<typeof Bun.spawn> | null,
  server: null as BunServer<unknown> | null,
  serverUrl: null as string | null,
  rl: null as ReadlineInterface | null,
}

export function requireBinary(name: string) {
  let path = resolveBinary(name)
  if (path) return path
  console.error(`Could not find ${name} binary.`)
  console.error("Build from source: cd engine && make build")
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

export function broadcastStop() {
  for (let ws of state.clients.keys()) {
    ws.send(JSON.stringify({ type: "stop" }))
  }
}

export function shutdown() {
  if (state.child) state.child.kill()
  if (state.server) state.server.stop()
  process.exit(0)
}