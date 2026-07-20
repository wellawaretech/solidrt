import { state, print, requireBinary, pipeAbovePrompt, shutdown } from "./util"
import { DEV_HOST, DEV_PORT, getClients, shutdownWhenEmpty } from "./dev-server"
import { values, clientStorageArgs } from "./args"

export function spawnClient() {
  let runner = requireBinary("solidrt-go")
  // The local client and dev server share this machine, so connect straight to
  // the loopback server: no mDNS discovery or recents lookup is needed for `run`.
  let args: string[] = ["--dev-server", `${DEV_HOST}:${DEV_PORT}`, ...clientStorageArgs()]
  if (values.size) args.push("--size", values.size)
  state.child = Bun.spawn([runner, ...args], {
    stdio: ["ignore", "pipe", "pipe"],
  })

  if (state.child.stdout && typeof state.child.stdout !== "number")
    pipeAbovePrompt(state.child.stdout, process.stdout)
  if (state.child.stderr && typeof state.child.stderr !== "number")
    pipeAbovePrompt(state.child.stderr, process.stderr)

  state.child.exited.then(async () => {
    let clients = await getClients().catch(() => [])
    if (clients.length === 0) {
      shutdown()
    }
    print(`[cli] Local client exited, ${clients.length} remote client(s) still connected`)
    // From here, exit once the last remote client disconnects.
    shutdownWhenEmpty()
  })
}
