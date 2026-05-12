import { DEV_HOST, DEV_PORT, state, print, requireBinary } from "./util"

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
  let runner = requireBinary("solid-rt-runner")
  state.child = Bun.spawn([runner, "--dev", "--dev-server", `${DEV_HOST}:${DEV_PORT}`], {
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
    print(`[dev] Local client exited, ${state.clients.size} remote client(s) still connected`)
  })
}