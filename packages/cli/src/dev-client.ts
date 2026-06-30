import { state, print, requireBinary } from "./util"
import { DEV_HOST, DEV_PORT } from "./dev-server"
import { values } from "./args"

function pipeAbovePrompt(stream: ReadableStream<Uint8Array>, out: NodeJS.WriteStream) {
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

export function spawnClient() {
  let runner = requireBinary("solidrt-go")
  // The local client and dev server share this machine, so connect straight to
  // the loopback server: no mDNS discovery or recents lookup is needed for `run`.
  let args: string[] = ["--dev-server", `${DEV_HOST}:${DEV_PORT}`]
  if (values.size) args.push("--size", values.size)
  state.child = Bun.spawn([runner, ...args], {
    stdio: ["ignore", "pipe", "pipe"],
  })

  if (state.child.stdout && typeof state.child.stdout !== "number")
    pipeAbovePrompt(state.child.stdout, process.stdout)
  if (state.child.stderr && typeof state.child.stderr !== "number")
    pipeAbovePrompt(state.child.stderr, process.stderr)

  state.child.exited.then(() => {
    if (state.clients.size === 0) {
      state.server?.stop()
      process.exit(0)
    }
    print(`[cli] Local client exited, ${state.clients.size} remote client(s) still connected`)
  })
}